# Review prompt — the standard template

Owner ruling 2026-08-29. This is the exact brief that reviewed N1 and came back with three blockers, eight serious findings and nine minor ones, several of which the implementer's own reading had missed. Every review starts from this file.

What made it work, and must survive any editing:

- It named the project rules the code has to answer to, by section, instead of asking for "a review".
- It listed the specific failure classes to hunt, in order of what they would cost.
- It demanded the dependency sources be read rather than trusted: the threading verdict was reached by reading `rfd`, `tauri-plugin-dialog` and `tauri-runtime-wry`, not by believing the implementer's docstring.
- It put the tests under review alongside the code, which is where three empty assertions and a branch that passed on a missed click were found.
- It refused a clean bill of health as an answer: "if you found nothing you did not look hard enough".

## Two additions, mandatory since 2026-08-29

**The report goes to a file.** The brief below must always carry a report path under `docs/reports/`, and must say that the reviewer writes the report there before finishing. The caller reads that file and never the closing message. A review whose report file is missing or empty is a failed review, whatever the closing message claims. This rule exists because a three-blocker review nearly passed for "the agent produced nothing": the report was alive in the transcript and dead in the result.

**Reviews are always delegated.** The implementer's own reading of their diff never satisfies the requirement, however careful it was.

## The template

Replace the branch name, the change description, the file list and the specific hunt list. Keep everything else, including the closing refusal.

---

Sei il revisore di un cambiamento appena scritto nel repo /home/alcahest/git/SubLore, branch `n1-close-gate`. NON modificare nessun file: produci solo il verdetto.

Il diff da rivedere è `git diff main...HEAD` più le modifiche non committate (`git diff` e i file untracked). Guardali tutti con git.

COSA FA IL CAMBIAMENTO. Aggiunge un "close gate": chiudendo la finestra con modifiche non salvate compare un dialogo nativo con Salva / Scarta / Annulla, invece di buttare via il lavoro in silenzio. File toccati: src-tauri/src/lib.rs, src-tauri/src/subtitle/mod.rs, src-tauri/src/strings.rs, e2e/lib/input.js, e2e/scripts/close-gate-check.js (nuovo), package.json.

REGOLE DEL PROGETTO che il codice deve rispettare, leggile in CLAUDE.md:

- §3 sicurezza dati: un bug può costare fastidio, mai dati. Scritture atomiche, backup mai cancellati da logica automatica, il file dell'utente mai toccato senza che l'utente lo abbia chiesto.
- §6 qualità: niente unwrap fuori dai test, errori gestiti ai confini, commenti massimo 1-2 righe, niente codice morto o astrazioni speculative.
- §5.4: mai indebolire un test.
- §7 budget prestazioni.

CERCA IN PARTICOLARE, con severità:

1. Percorsi di perdita dati. Il gate salva o scarta: se il salvataggio fallisce, la finestra deve restare aperta? Il codice lo fa davvero? Cosa succede se il dialogo viene chiuso dal window manager invece che con un bottone?
2. Deadlock e thread. Il dialogo parte da dentro l'handler del run loop di Tauri. `project::choose_path` documenta che i dialoghi bloccanti sul main thread mettono in deadlock l'app. Il nuovo codice usa `show_with_result` (callback, non bloccante) e poi `run_on_main_thread` per la distruzione della finestra. Verifica che non ci sia un percorso che blocca il main loop o che tocchi la superficie video nativa fuori dal main thread.
3. Cicli. Se l'utente sceglie Scarta, la sessione viene chiusa con discard; se sceglie Salva, viene marcata pulita. L'intento è che la chiusura successiva non ritrovi niente di sporco e non possa entrare in loop di dialoghi. Verifica che regga davvero, incluso il caso in cui il salvataggio fallisca.
4. Poisoning del mutex: `is_dirty` risponde `true` su lock avvelenato. È la scelta giusta o nasconde un problema?
5. Il test e2e/scripts/close-gate-check.js: è onesto? Le sue asserzioni verificano comportamento vero o si accontentano di proxy deboli? Le attese fisse (sleep) lo rendono fragile in CI? Il conteggio dei controlli protegge da asserzioni rimosse?
6. Qualunque cosa io abbia rotto negli altri consumatori: `SubtitleState::slot` e `backup_root` sono diventati pubblici, `close_session(discard: true)` viene ora chiamato da un secondo posto.

Verdetto per ogni rilievo: gravità (bloccante / serio / minore), file:riga, e la correzione consigliata. Se non trovi nulla di bloccante dillo chiaramente, ma se non hai trovato NIENTE non hai guardato abbastanza a fondo.

Scrivi il rapporto completo in `docs/reports/<TASK>-review.md` PRIMA di terminare. Il tuo messaggio di chiusura non e il rapporto: e il file che conta, e un file mancante o vuoto vale come review fallita.
