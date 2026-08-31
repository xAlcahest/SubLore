# Gate 2 — closing report

`GATE_BASE=f0b0058` · `GATE_HEAD=eca9806` · opened 2026-08-30, seven merged deliveries plus N2c, none of which had a dedicated review by choice of regime.

**Verdict: the gate cannot open yet, and not for an engineering reason.** The second closure audit re-derived all 59 rows independently and closed 51, several by mutation-testing the fix itself — reintroducing the guard-in-scrutinee deadlock and watching the invariant test go red. One row is owner-ruled by the standing platform policy. **Seven are open, and four of those are sound implementer arguments waiting on a decision that has never been recorded.** The plan's exit condition asks for every finding to be fixed _or explicitly ruled on by the owner_; these are the second kind, and only the owner can supply them.

Two of the seven were the orchestrator's and are now closed: a report naming the probe by the filename it had in a scratch directory, and citation drift in `decisions.md` reopened by a concurrent edit.

## What the gate still needs from the owner

1. **`lib.rs:192` — an answer worker that blocks forever holds the window shut.** `save_current` takes the session lock deliberately; if another command never releases it, the gate stays in `Acting` for the life of the process and the window cannot close. The implementer added no timeout and argued that every automatic release is worse than the wedge: letting the close through discards the work the gate protects, and raising a second dialog puts two saves on one session. The argument is sound and it is not a decision an implementer can take.
2. **`main.rs:14` and `:26` — the NVIDIA signal is broad on purpose.** `/sys/module/nvidia` answers "is the module loaded", not "is NVIDIA drawing". The implementer measured that narrowing would work on this machine and refused to narrow anyway: on a PRIME-offload laptop the panel hangs off the iGPU while NVIDIA renders the webview, and the narrow signal would disarm the mitigation and turn a latency cost into a blank window. The sysfs data to overrule that is in `gate2-fix-env.md`.
3. **`main.rs:27` — the mitigation's cost was never measured against a budget.** CLAUDE.md section 7 sets cold start under 2 s and idle memory under 400 MB. The armed-versus-disarmed latency was measured under Xvfb, where the renderer is llvmpipe and the mitigation does not ship. Cold start and idle PSS on the real display, armed and disarmed, are owed — or a ruling that they wait.
4. **`main.rs:37` — nothing pins the toolchain.** There is no `rust-toolchain.toml`; the edition is pinned in `Cargo.toml` and nothing else is. Either a file lands or the row is ruled a non-goal.

Rows 2, 3 and 4 are all in the mitigation's neighbourhood, which is the code the owner promoted to blocker and the code a real run now exercises. The engineering there is done; what is missing is a signature.

## What the gate did

| wave          | shape                                            | outcome                                                                                                         |
| ------------- | ------------------------------------------------ | --------------------------------------------------------------------------------------------------------------- |
| 0             | freeze, GATE_HEAD recorded, reference battery    | green, `gate2-battery-baseline.md`                                                                              |
| 1             | twelve lenses in one parallel wave               | 72 findings → 59 register rows: 2 blockers, 26 serious, 31 minor                                                |
| 2             | dedup and triage, orchestrator, not delegated    | `gate2-register.md`; no lens contradicted another, so no adjudication was needed                                |
| 3 round one   | six implementers, file-disjoint, none the author | fixes across 25 files; two findings ruled **not a defect** with argument, several left unfixed with reasons     |
| consolidation | merge two duplicate checks, wire CI              | one check of 6 assertions from 4+3; two checks left out of CI **with the reason recorded in the workflow file** |
| 4 round one   | two delegates, neither a fixer                   | **a blocker created by a fix**: a self-deadlock in the close gate, reproduced with standalone programs          |
| 3 round two   | four implementers, parallel                      | the deadlock, a reverted narrowing, the silent argument drop, the two not-closed rows                           |
| 4 round two   | two delegates, independent re-derivation         | pending                                                                                                         |

## The two blockers Wave 1 found, and where they came from

Both were in code written the same day as the gate, by the orchestrator.

- `src-tauri/src/lib.rs:75` — `std::env::args()` panics on an argument that is not valid Unicode, and a Linux filename is an arbitrary byte string. `sublore <non-utf8-name>` killed the app before its window existed. Found independently by L1 and L2. Closed with `OsString`, proved by `startup-args-check.js`, 7 assertions.
- `src-tauri/src/lib.rs:138` — the `CLOSING` arm ran before the dirty check, so an edit committed while the gate's save was in flight was closed away in silence. CLAUDE.md section 3. Found by L2 and L5. Closed, proved by `close-gate-late-edit-check.js`, 8 assertions, and the app now logs "main was edited after its gate was answered, asking again".

The owner's named lens — the close path and the single-use `CLOSING` flag — is where the second one was found. It was named in advance because that code is adjacent to data safety and deserved eyes that were not its author's, and it returned exactly that.

## The finding the owner promoted

L7 established that the shipping configuration was exercised by no path that runs anywhere: `e2e/lib/env.js` disarms the mitigation for the whole suite, so every automated run tested a configuration no user gets. Promoted to blocker by owner ruling, with a real run required rather than a test that observes.

Measured on the owner's display, after `main.rs` changed in round two and re-run for that reason:

```
armed:    luma 16..235 (range 219), painted, 1749 ms to first painted capture
disarmed: luma 46..46  (range 0),   never painted
```

The mitigation is what makes the window paint on that machine. A shipped mitigation nothing exercises is indistinguishable from a broken one; it is now exercised and asserted.

## The battery, against Wave 0's baseline

|                                   | Wave 0            | after round two   |
| --------------------------------- | ----------------- | ----------------- |
| cargo test                        | 502               | **538**           |
| wdio specs                        | 8                 | 8                 |
| shutdown · close gate · scale     | 5/5 · 12/12 · 5/5 | 5/5 · 12/12 · 5/5 |
| startup-args                      | did not exist     | 7/7               |
| close-gate-late-edit              | did not exist     | 8/8               |
| wayland ×3                        | 4/4               | 4/4               |
| webview-paint on the real display | did not exist     | 5/5               |

## What the gate cost, and what it caught that the suite could not

Three things were green throughout and wrong: the self-deadlock, the silent argument drop, and the mitigation nobody ran. None of them is visible to a passing suite, which is the argument for the gate existing at all.

Two harness lessons were paid for in false reds and are now rules in WORKFLOW section 4c: a battery must not reuse an `xvfb-run` display number, and a discrimination experiment must check that its rebuild happened before it measures.

## Residue, filed rather than fixed

By owner ruling the gate opens on blockers and serious, not on minor perfection: a gate that demands minor perfection becomes a wall, and walls get walked around. `N5` carries the minors that were not co-located with a fix, each with the register row that originates it. `N6` carries something Wave 4 found while reading for something else: the close gate protects `CloseRequested` and nothing else, so the moment M2.0's T7 adds a menu with a Quit item, unsaved work leaves with the process.

## The ordering decision the owner still owes: N1c

`docs/design/m2-0-tasks.md` poses it and does not settle it. The picker still goes through the dialog plugin (`project/mod.rs:244-257`), and T2 turns one plugin call site into four.

- **Option A, as written in the plan.** N1c first, T2 inherits a GTK-direct picker. T1's by-title lookup is re-validated against the new picker before T2 begins.
- **Option B, as written in the plan.** T2 builds the four choosers on GTK directly and closes N1c in the same delivery.
- **Option C, recommended.** N1c immediately after the gate opens, **before T1**. T1 builds the harness helper that identifies the chooser window by title, and that helper assumes an rfd/GTK3 toplevel. If the chooser changes after T1, the helper needs re-validating; if it changes before, T1 is built once against the final thing and T2 inherits it with no rework. Cost: one delivery before T1 starts. Benefit: no re-validation of T1, no doubling of T2.

## One open observation, recorded rather than swept

On 2026-08-31, the first run of the wdio suite after the toolchain was pinned to stable 1.93.0 — the first run against a completely rebuilt binary — reported **7 spec files passed, 1 failed**. Four consecutive runs since have been 8 of 8, and every script check passed in the same battery.

**Quale spec fallisse allora non è noto**, and that is a gap in the battery command rather than in the suite: it extracted the summary line and not the failing name, so the one run that mattered left no record of itself. The suite's own output would have named it; the command threw it away.

So: one unexplained failure in five runs, no cause, no reproduction, and no claim that it is understood. It is not presented as fixed and not presented as flakiness — it is presented as unexplained. If it returns, the first thing to do is capture the run's full output rather than its summary.

## Coda: la CI, 2026-08-31

Il cancello si era chiuso con la suite verde in locale e la CI rossa, che è la combinazione che questo
documento esiste per non lasciar passare. Ora sono verdi entrambe, e per motivi diversi da quelli che
sembravano.

| job                  | prima                                       | ora                                 |
| -------------------- | ------------------------------------------- | ----------------------------------- |
| `check (ubuntu)`     | verde                                       | verde                               |
| `check (windows)`    | 3 target morti al caricamento, `0xc0000139` | **verde**, 515 test                 |
| `e2e smoke (ubuntu)` | rosso su un save che non c'era              | verde, 8/8 spec e ogni script pieno |

Windows: i binari di test di cargo non ricevevano il manifest che `tauri_build` dà all'applicazione,
quindi `comctl32` si risolveva sulla 5.82 di `System32`, che non esporta `TaskDialogIndirect`. Due
ipotesi prima di questa erano coerenti e sbagliate; sono in `docs/reports/windows-entrypoint.md`
insieme al motivo per cui il confronto delle tabelle statiche non poteva rispondere.

Linux: verificato scaricando l'artefatto dei log, non guardando le spunte. Gli step e2e hanno
`continue-on-error`, quindi si presentano verdi anche quando il comando è fallito, ed è esattamente
così che il 2026-08-30 ho riportato all'owner "sono passati tutti" mentre non era vero.

## Coda: una regola scritta qui e rotta il giorno dopo

WORKFLOW §4c dice dal 2026-08-30 che l'input sintetico va solo dentro un server X che l'harness
possiede. Il 2026-08-31 ho lanciato `pnpm e2e:close-gate` senza `xvfb-run` e xdotool ha digitato nella
sessione reale dell'owner. Nessuno se n'è accorto: il check è fallito, e il fallimento sembrava un
difetto del prodotto.

`e2e/lib/input.js` adesso chiede a `$DISPLAY` se c'è un window manager prima di ogni `xdotool`, e si
rifiuta se c'è. Provato in tutte e due le direzioni: rifiuta sul display reale, passa sotto Xvfb.

La lezione non è "stai più attento". È che una regola che vive solo in un documento viene rotta da chi
l'ha scritta, e la prima volta che serve a qualcosa è quando ha i denti.

## Coda: la spec sconosciuta ha un nome

L'osservazione qui sopra — una corsa su cinque rossa, causa ignota — si è ripresentata in CI il
2026-08-31 e stavolta il log c'era: `cue list editing > scrolls a viewport at a time without falling
behind`, il budget di scroll di M2.3.

I numeri: `mean 171.4 ms, max 3130.0 ms` contro un'allowance di 32 ms. Tolto il singolo passo da
3130 ms, gli altri diciannove fanno 15.7 ms di media, cioè esattamente quello che fa la macchina
dell'owner. Il runner non è lento a disegnare: si è fermato una volta, per tre secondi, e la media
era ostaggio di quella pausa.

Cosa è cambiato nel test, detto esplicitamente perché è il confine del §5.4:

- **Mediana al posto della media** per il budget centrale. La media su venti campioni su una VM
  condivisa la decide lo stallo peggiore; la mediana risponde alla domanda che dà il nome al test.
- **Penultimo al posto del massimo** per il caso peggiore. Uno stallo su venti è la macchina, due
  sono il codice. Il massimo viene loggato, non asserito, così una regressione vera resta visibile
  nella corsa che la trova.
- **Un passo che non ha mai mosso la lista adesso fallisce.** L'asserzione precedente diceva di
  fare questo e non lo faceva: controllava che il tempo fosse positivo, e un passo che esaurisce i
  400 tentativi di `settle` ha un tempo positivo come tutti gli altri. Questa parte è più severa di
  prima, non meno.
- **Il log stampa tutti e venti i tempi**, perché la corsa che conta non lasci di nuovo solo un
  riassunto.

Che non sia un indebolimento è stato provato, non affermato: rimossa la virtualizzazione in
`CueList.tsx` (`first = 0`, `last = count`), il test fallisce ancora. Rimessa, passa tre volte su
tre con mediana 8–16 ms contro un'allowance di 24.
