# N1 close gate — seconda passata di review

Branch `n1-close-gate`, revisionato contro `main` il 2026-08-29.
Ambito: `git diff main` più i file nuovi, verificati con git, non dall'elenco del brief.

## Prove raccolte in questa passata

| Cosa                            | Comando                                   | Esito                                                                                                                                                                              |
| ------------------------------- | ----------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Compilazione, test inclusi      | `cargo check --tests`                     | exit 0                                                                                                                                                                             |
| Lint                            | `cargo clippy --tests -- -D warnings`     | exit 0, nessun warning                                                                                                                                                             |
| Suite Rust                      | `cargo test --test subtitle_editing`      | 17 passati, 0 falliti (i 4 nuovi inclusi)                                                                                                                                          |
| Sorgenti delle dipendenze letti | `tauri-plugin-dialog-2.7.2`, `rfd-0.16.0` | vedi rilievi 1 e verifiche META 1                                                                                                                                                  |
| Suite e2e                       | **non eseguita**                          | nessun display e nessun binario: `src-tauri/target/debug/sublore` non esiste. Tutto ciò che dico sui percorsi e2e viene dalla lettura del codice, non da una corsa (CLAUDE.md §9). |

Riepilogo: **1 bloccante, 5 seri, 9 minori.**

---

## META 1 — le correzioni della prima passata reggono?

### 1. Salvataggio fallito mostra un secondo dialogo — **REGGE**

`src-tauri/src/lib.rs:216-221` intercetta l'errore, lo logga e chiama `report_save_failure`
(`lib.rs:226-232`), che alza un `MessageDialogKind::Error` nativo. Il ritorno `false` porta al ramo
`else` del callback (`lib.rs:196-199`), che tiene la finestra aperta e riabbassa `GATE_OPEN`.
Il comportamento è quello dichiarato. Riserva sul **testo** del dialogo: vedi rilievo 2.

### 2. Recupero dal mutex avvelenato — **L'AFFERMAZIONE È VERA, IL COMMENTO LA SOTTODICHIARA**

L'affermazione da verificare era: «`plan::edit` costruisce un documento nuovo e `EditSession::commit`
lo assegna in un colpo, quindi una panic lascia un documento intero o l'altro, mai mezzo».

Verificata nei sorgenti:

- `crates/sublore-edit/src/plan.rs:129-145` — `edit()` fa splice sui byte, ri-parsa in `after`, verifica,
  e restituisce `Edited { document: after, ... }`. Il `document` in ingresso è `&SubtitleDocument`:
  non viene mai mutato. Il commento a `plan.rs:127-128` lo dichiara e il codice lo rispetta.
- `crates/sublore-edit/src/session.rs:135-142` — `commit()` ha come prima istruzione
  `self.document = document;`. È un'assegnazione di un valore già costruito, non una mutazione in place.

Quindi il campo `document` non può essere mezzo scritto e `save_current` non può scrivere byte
incoerenti sul file dell'utente. **La giustificazione tiene: il gate non è peggio del difetto originale.**

Ma la frase è più stretta di quello che serve, e omette il pezzo che conta davvero:

- `session.rs:89-91` — in `apply()`, `self.history.record(...)` gira **prima** di `commit`.
- `session.rs:97` e `session.rs:112` — in `undo()`/`redo()`, il cursore della history si muove **prima**
  del replay.

Un panic in mezzo lascia `history`, `views` e `revision` disallineati dal `document`. Ho tracciato le
conseguenze reali:

- Panic dentro `plan::edit` (il sito plausibile: `document.slice(span)` panica su uno span fuori range):
  niente è stato ancora toccato. `save_current` scrive il documento pre-edit, coerente con `dirty()`.
- Panic dentro `commit` dopo l'assegnazione (in `diff::views`/`diff::patch`): documento nuovo,
  `views` e `revision` vecchi. `save_current` scrive il documento nuovo, cioè l'ultima modifica reale
  dell'utente.

In entrambi i casi i byte su disco sono un documento intero e sensato. La finestra in cui si
scriverebbe qualcosa di sorprendente sta fra `history.record` e `self.document = document`: zero
istruzioni. **Nessun rilievo bloccante.** Il commento va però reso onesto (rilievo minore 6): l'invariante
che salva è «il documento è sempre un documento intero», non «una panic lascia l'uno o l'altro», che è
vero per il campo e falso per la sessione.

### 3. `session_state` con `try_lock` — **REGGE COME MECCANICA, MA APRE IL BLOCCANTE**

`src-tauri/src/subtitle/mod.rs:411-419` non blocca mai. Chiamato una sola volta per richiesta di
chiusura (`lib.rs:96`), non in un loop e non per frame: **nessun problema di §7**, il costo è un
`try_lock` O(1) sul main loop. Il test `session_state_answers_unknown_rather_than_waiting_for_a_held_lock`
(`src-tauri/tests/subtitle_editing.rs`) lo prova e passa.

Ma la conseguenza a valle di `Unknown` è il bloccante 1 qui sotto.

### 4. `GATE_OPEN` e `.parent()` — **UNO REGGE, L'ALTRO NO**

`GATE_OPEN` (`lib.rs:144`, `lib.rs:100`) fa il suo lavoro: `swap(true)` è atomico, la seconda richiesta
di chiusura non alza un secondo dialogo. `.parent(&window)` invece è un no-op sulla piattaforma
che questo branch testa: rilievo serio 1.

### 5. `save_locked` condiviso — **REGGE, NESSUN RILIEVO**

`mod.rs:387-396`. Prende `&mut EditSession` sotto una guardia che il chiamante ha già in mano.
Chiamato da `save` (`mod.rs:382`, dopo `check_revision`) e da `save_current` (`mod.rs:437`, senza).
Un solo lock per percorso, nessun doppio lock, nessuna finestra fra il controllo di revisione e la
scrittura. La correzione è pulita.

### 6. Test e2e a prova positiva — **PARZIALE**

Prova positiva reale: `waitForDialogGone` (`e2e/scripts/close-gate-check.js:98-108`) lancia se il
dialogo non sparisce, ed è chiamato dopo ogni risposta. Un click che manca il bottone giusto viene
comunque colto a valle (discard e save che finiscono su Cancel fanno scadere `reap`; save che finisce
su discard fa fallire il confronto blocco per blocco). Questa parte è solida.

Non regge: le tre `check(..., true)` che accompagnano quelle attese (rilievo serio 3) e la precondizione
mai provata (rilievo serio 4).

Annulla via Escape: **verificato end-to-end nei sorgenti delle dipendenze.**
`rfd-0.16.0/src/backend/gtk3/message_dialog.rs:212` mappa `GTK_RESPONSE_DELETE_EVENT` a `Cancel` sul
percorso asincrono, e `tauri-plugin-dialog-2.7.2/src/desktop.rs:248-251` lo rimappa a
`Custom("Cancel")` per un set `YesNoCancelCustom`. Il catch-all dell'app (`lib.rs:192`) lo tratta come
"non chiudere". Corretto.

Nota di verifica correlata, perché era l'assunzione più rischiosa dell'intero cambiamento: il commento a
`lib.rs:181-183` dichiara che il plugin riscrive ogni bottone di un set custom in `Custom(label)`.
**È vero.** `desktop.rs:228-253` fa esattamente quel rimapping (`Yes`→`Custom(yes)`, `No`→`Custom(no)`,
`Cancel`→`Custom(cancel)`), con il commento della crate «on Linux rfd does not return
`Custom`, so we must map manually». Se fosse stato falso, Salva e Scarta sarebbero caduti entrambi nel
catch-all e il gate sarebbe stato una trappola che non chiude mai. Non lo è.

Salvataggio verificato blocco per blocco: sì, `close-gate-check.js:333-346`, e non «i byte sono cambiati».
Esistenza del backup: sì, `:347-351` (ma vedi minore 10 e serio 5).

### 7. `shutdown-check.js` prova che su pulito nessun dialogo appare — **REGGE**

`e2e/scripts/shutdown-check.js:84-90` campiona la lista delle finestre X ogni 100 ms durante la chiusura,
`:99-105` asserisce che il gate non è mai apparso. Doppia copertura: se il gate apparisse su un documento
pulito l'app non uscirebbe affatto e il `waitFor` a `:93` scadrebbe prima. Il campionamento parte poco
prima di una `execFileSync` che blocca l'event loop, ma il dialogo del gate resterebbe su finché non
risposto, quindi la finestra persa non conta. **Nessun rilievo.**

### 8. Test in CI e file tracciati — **PARZIALE, VEDI SERIO 5**

`.github/workflows/ci.yml:194-195` e `package.json:17` ci sono. `e2e/scripts/close-gate-check.js` e
`docs/reviews/review-prompt.md` sono tracciati (`git ls-files` conferma). Ma il working tree ha quattro
file modificati **non committati**, e uno è una correzione necessaria: rilievo serio 5.

---

## Rilievi

### BLOCCANTE

**[BLOCCANTE] `src-tauri/src/subtitle/mod.rs:428-438` — il gate scrive sul file dell'utente anche quando il documento non è sporco, e sovrascrive modifiche esterne senza controllarle.**

Il difetto: `save_current` non guarda `dirty()` e non guarda la revisione. Salva sempre.
Combinato con `session_state` che risponde `Unknown` (`mod.rs:417`) e `unsaved_work` che tratta
`Unknown` come sporco (`lib.rs:151-156`), il gate può chiedere di salvare un documento pulito, e la
risposta Salva scrive.

Lo scenario concreto in cui morde: il lock è tenuto per tutta la durata di `read_document`
(`mod.rs:289-300`, apertura + parse fino a 16 MB) e per tutta `save_with_backup` (`mod.rs:392`).
L'utente preme Ctrl+S, o clicca Open sul fixture da 2000 cue, e subito dopo preme la X. `try_lock`
fallisce, `Unknown`, il dialogo dice «The subtitle file has edits that are not on disk» su un documento
che non ne ha. L'utente, ragionevolmente, clicca Salva. `save_current` blocca sul lock, lo ottiene, e
riscrive il file più un backup nuovo.

Perché è §3 e non solo fastidio: CLAUDE.md §3.1 dice che il file dell'utente non viene toccato senza
che l'utente lo abbia chiesto, e §3 chiude con «un bug può costare fastidio, mai dati». Il contenuto
scritto è byte-identico (il round-trip è provato dalla suite), quindi non si corrompe niente. Ma:

- cambia la mtime di un file che l'utente ha solo aperto, cosa che rompe chi sincronizza o versiona;
- accumula un file di backup per un'operazione che non è mai stata chiesta;
- e soprattutto: **se il file su disco è stato modificato da un altro programma da quando è stato aperto,
  `save_current` lo sovrascrive con la copia in memoria, senza controllo di revisione, senza controllo
  di mtime e senza chiedere.** La modifica esterna finisce nel backup, quindi è recuperabile, ma è
  perdita di lavoro altrui provocata da un dialogo che non doveva neanche apparire.

La correzione, una riga: in `save_current`, dopo `current(&mut guard)?`, restituire senza scrivere se
`!session.dirty()`. Il gate ha già la risposta giusta per quel caso — non c'è niente da salvare — e
`SubtitleSaved` può portare `bytes_written: 0`, oppure la firma diventa
`Result<Option<SubtitleSaved>, SubtitleError>`. In alternativa, o in aggiunta, rendere `unsaved_work`
meno grossolano: `Unknown` può restare "chiedi", ma la risposta Salva non deve essere una scrittura
incondizionata.

### SERI

**[SERIO] `src-tauri/src/lib.rs:176-180` — `.parent(&window)` non fa niente su Linux, e la giustificazione scritta accanto al guard di rientranza si regge su di esso.**

Il difetto: il commento dice «Owned by the window, so it cannot be lost behind it and invite the second
close request the re-entrancy guard above then has to refuse». Falso sulla piattaforma che questo branch
costruisce e testa. `rfd-0.16.0/src/backend/gtk3/message_dialog.rs:85` costruisce il dialogo con
`gtk_message_dialog_new(ptr::null_mut(), ...)` e quel file **non legge mai** `opt.parent`: un grep di
`parent` su `src/backend/gtk3/message_dialog.rs` restituisce zero occorrenze, e `set_parent` esiste solo
in `src/message_dialog.rs:72` e in `file_dialog.rs`. Il campo viene memorizzato e ignorato.

Lo scenario in cui morde: sotto un window manager reale, un toplevel GTK senza transient-for non viene
tenuto sopra la finestra che lo ha generato. L'utente lo perde dietro, ripreme la X sulla titlebar — che
passa dal WM, non da GTK — `CloseRequested` scatta, `api.prevent_close()` viene chiamato,
`GATE_OPEN.swap(true)` restituisce già `true` (`lib.rs:100`) e **non succede assolutamente niente, senza
alcun segnale**. L'app rifiuta di chiudersi in silenzio. Attenuante reale: il dialogo è creato con
`GTK_DIALOG_MODAL` (`message_dialog.rs:87`), quindi l'input alla finestra principale è bloccato e
l'utente capisce che qualcosa è aperto. Non è perdita di dati, ma è la modalità di fallimento «finestra
non chiudibile e muta» che il brief chiedeva di cercare, ed è documentata come impossibile da un commento
sbagliato. In CI non si vede: xvfb gira senza window manager.

La correzione: togliere `.parent()` e il commento che promette quello che non fa, e coprire il buco dove
è reale — alzare la finestra e portarla a fuoco quando una seconda `CloseRequested` arriva con
`GATE_OPEN` già alzato, invece di scartarla in silenzio. Due righe nel ramo `else` implicito di
`lib.rs:100`.

**[SERIO] `src-tauri/src/subtitle/mod.rs:432-435` e `src-tauri/src/strings.rs:41-46` — dopo `into_inner()` la sessione resta avvelenata, e il messaggio d'errore manda l'utente su una strada che non può funzionare.**

Il difetto: `PoisonError::into_inner()` estrae la guardia ma **non azzera il flag di poison** del mutex.
Il recupero vale per quella singola chiamata. Ogni comando successivo passa da `lock()`
(`mod.rs:518-525`), che rifiuta.

Lo scenario in cui morde, che è esattamente la domanda «cosa vede l'utente dopo un Annulla su lock
avvelenato»: un comando è morto tenendo il lock. L'utente preme X. `session_state` fa `try_lock`, che su
un mutex avvelenato restituisce `Err`, quindi `Unknown`, quindi il dialogo. L'utente clicca Annulla.
La finestra resta, e da quel momento **ogni** azione — Salva dalla toolbar, Salva copia, ogni edit,
undo, redo — fallisce con «the subtitle session lock is poisoned». L'unica via che funziona ancora è
ripremere la X e scegliere Salva, e niente nell'interfaccia lo dice.
Stessa cosa dopo un Salva fallito: `close_save_failed` (`strings.rs:44`) dice testualmente «Try Save from
the toolbar, or Save copy to another location», e sotto poison entrambe sono garantite fallire. È un
consiglio impossibile dato all'utente nel momento in cui ha lavoro non salvato in ballo (§9).

Non è bloccante perché richiede due guasti in fila (una panic, poi un salvataggio che fallisce comunque)
e il lavoro resta in memoria e recuperabile via X→Salva. Ma è la stessa classe del rilievo che la prima
passata aveva chiamato «un percorso di salvataggio che non poteva mai riuscire», spostata di un passo.

La correzione: chiamare `slot.clear_poison()` dopo il recupero, così il recupero è completo e non solo
per una chiamata. È stabile da Rust 1.77 e il toolchain qui è 1.95-nightly con edition 2021, quindi è
disponibile. Se invece si vuole deliberatamente tenere la sessione marchiata, allora `close_save_failed`
deve dire la verità e indicare la X come via di uscita.

**[SERIO] `e2e/scripts/close-gate-check.js:267, 284, 322` — tre asserzioni che asseriscono la costante `true`, contate dal guard che dovrebbe impedirlo.**

Il difetto: `check("cancel closed the dialog", true)`, `check("discard closed the dialog", true)`,
`check("save closed the dialog", true)`. La verifica vera è `await waitForDialogGone(...)` sulla riga
sopra, che lancia. Le tre `check` non asseriscono niente: incrementano solo `checksRun` verso
`EXPECTED_CHECKS = 14` (`:38`).

Lo scenario in cui morde: qualcuno cancella la riga `await waitForDialogGone("save")`. Il test resta
verde a 14/14, e il ramo Salva torna a passare senza aver mai provato che la risposta ha raggiunto un
bottone — cioè il difetto che l'intestazione del file a `:11-14` dichiara di aver eliminato per regola
dell'owner. Il commento a `:37` dice «Gutting an assertion has to be as red as failing one»: qui non lo è.
Peggiora il fatto che `docs/reviews/review-prompt.md:11` registra questo identico difetto come già trovato
dalla passata precedente («three empty assertions»). Non è stato corretto: è stato conservato e conteggiato.

La correzione: far restituire `true` a `waitForDialogGone` e passarne il valore a `check`, per esempio
`check("save closed the dialog", await waitForDialogGone("save"))`. Cancellare la chiamata diventa allora
un errore di sintassi o un conteggio che non torna. Oppure eliminare le tre e portare `EXPECTED_CHECKS`
a 11, che è il numero di asserzioni reali.

**[SERIO] `e2e/scripts/close-gate-check.js:82-88` — il messaggio diagnostico attribuisce alla causa sbagliata il fallimento che coprirebbe la regressione peggiore.**

Il difetto: se l'app esce invece di chiedere, `waitForDialog` lancia con «The gate was not reached: the
setup did not leave the document dirty, so this run proves nothing about it.» Ma quel ramo copre due
cause opposte: il setup che non ha sporcato il documento (flake dell'harness), e **il gate che ha lasciato
chiudere un documento sporco**, cioè la regressione esatta che N1 esiste per impedire. Il test dichiara al
lettore che la corsa non prova niente proprio quando potrebbe aver appena colto il difetto.

Lo scenario in cui morde: le coordinate hardcoded `SUBTITLE_PATH_FIELD`, `SUBTITLE_OPEN_BUTTON`,
`FIRST_CUE_TEXT` (`:42-44`) portano un commento che dice «M2.0 must revisit these». Quando M2.0 sposta la
UI, ogni ramo fallirà con quel messaggio, e la lettura naturale sarà «harness rotto, alzo gli sleep».
Se nel frattempo il gate si è rotto davvero, nessuno lo saprà.

La correzione: provare la precondizione prima di `requestClose(toplevel)` (`:259`, `:314`) invece di
assumerla — leggere il marcatore dirty dalla UI o rileggere la riga della cue e verificare che porti
`EDIT_MARK` — e fallire lì con un messaggio proprio. Le due cause diventano distinguibili e il messaggio
può dire il vero.

**[SERIO] Working tree — quattro file modificati non committati, e uno è una correzione che il branch committato non ha.**

Il difetto: `git diff --stat` mostra `docs/reviews/review-prompt.md`, `e2e/README.md`,
`e2e/scripts/close-gate-check.js` e `e2e/scripts/shutdown-check.js` modificati e non committati.
Fra queste modifiche c'è `close-gate-check.js:222`, che corregge il percorso dei backup da
`path.join(dataHome, "sublore", "backups")` a `path.join(dataHome, "com.sublore.app", "backups")`.
L'identificatore corretto è `com.sublore.app` (`src-tauri/tauri.conf.json:5`).

Lo scenario in cui morde: mergiando lo stato committato del branch, `backupsUnder` guarderebbe una
directory che non esiste mai, restituirebbe `[]`, e il check «save kept a timestamped backup of what it
overwrote» (`:347-351`) fallirebbe sempre. Il primo run di CI dopo il merge sarebbe rosso su una riga
già corretta sul disco di chi ha scritto il codice.

La correzione: committare il working tree prima di considerare la delivery chiusa, e ricontrollare che
`git diff main` e `git diff main...HEAD` coincidano.

### MINORI

**[MINORE] `src-tauri/src/lib.rs:251-267` — un ramo lascia `GATE_OPEN` alzato per il resto della sessione.**
Sul percorso di successo il flag non viene riabbassato, ed è giusto: la finestra sta sparendo. Ma
`close_window` è best-effort. Se `handle.get_webview_window(&label)` (`:256`) restituisce `None`, la
closure ha già eseguito `asr::shutdown` e `shutdown_video` (`:254-255`), non distrugge niente, non chiama
`report_close_failure`, e `GATE_OPEN` resta `true`: ogni X successiva chiama `prevent_close()` e poi tace.
Oggi il ramo è praticamente irraggiungibile — una sola finestra webview in `tauri.conf.json:13` — ed è per
questo che è minore, non serio. È però l'unico punto del disegno in cui il flag resta alzato senza che
nessuno lo abbassi, e M2.6 apre un secondo documento. Correzione: un `else` che chiama
`report_close_failure(&handle, ...)`, o spostare il reset in un guard che scatta sul drop della closure.

**[MINORE] `src-tauri/src/subtitle/mod.rs:421-427` — il commento che giustifica il recupero dal poison sottodichiara l'invariante che lo rende sano.**
Come tracciato nella sezione META 1.2: l'affermazione sul campo `document` è vera, ma la sessione nel suo
insieme può restare disallineata (history, `views`, `revision`), e il commento non lo dice. Un lettore che
si fida della frase potrebbe concludere che dopo un panic la sessione è intera, e usarla per altro.
Correzione: dire che l'invariante è «il documento è sempre un documento intero, quindi i byte scritti sono
sempre un file valido», e aggiungere una riga sul fatto che revisione e cronologia possono essere avanti
o indietro rispetto a quel documento. Da fare in una o due righe, non nel saggio attuale da sette.

**[MINORE] `src-tauri/src/subtitle/mod.rs:44` e `:503` — allargamento dell'interfaccia pubblica del modulo.**
`SubtitleState::slot()` ora consegna pubblicamente l'`Arc<Mutex<Option<EditSession>>>` grezzo, cioè la
scorciatoia che aggira `check_revision`. `save_current` la usa per una ragione giusta, ma ora la può usare
chiunque, e §6 tratta l'interfaccia pubblica del modulo come stabile. Correzione: tenere `slot()` privato e
far prendere a `session_state` e `save_current` un `&SubtitleState` invece di un `&SessionSlot`;
`backup_root` può restare `pub(crate)`, il gate è nella stessa crate.

**[MINORE] Commenti oltre il limite di 1-2 righe (CLAUDE.md §6).**
`src-tauri/src/lib.rs:159-164` (sei righe), `:181-183` (tre righe, commento inline sopra uno statement),
`:245-250` (sei righe); `src-tauri/src/subtitle/mod.rs:402-405` (quattro righe su una variante di enum),
`:421-427` (sette righe). I `///` su funzioni pubbliche sono rustdoc e la regola punta ai commenti inline
sulle guardie, ma `lib.rs:181-183` e `mod.rs:402-405` cadono dentro la regola per come è scritta, e i doc
comment di `save_current` e `close_window` sono rationale che §6 dice di mettere nella descrizione della
delivery. Correzione: una riga per il cosa e il perché con il riferimento a N1, il resto nella delivery.

**[MINORE] `BACKLOG.md`, voce N1, secondo bullet — la frase del difetto originale è rimasta incollata sulla riga del "Known gap".**
Un item ora `[x]` continua a dichiarare «**This is an active data-loss defect, not a missing feature:**
there is no `prevent_close` in the repo, dirty state is tracked on both sides and nobody consults it on
close, so closing the window throws unsaved edits away silently». `prevent_close` c'è, `lib.rs:97`.
È un residuo di copia-incolla che lascia il backlog a dire il falso su codice appena scritto (§9).
Correzione: chiudere il bullet del "Known gap" dopo «Out of scope here.» e cancellare il resto.

**[MINORE] `BACKLOG.md` dice «10 checks», lo script ne dichiara 14, il README ne dichiara 14.**
`BACKLOG.md` (bullet di status N1) contro `e2e/scripts/close-gate-check.js:38` (`EXPECTED_CHECKS = 14`) e
`e2e/README.md` (riga della tabella per `scripts/close-gate-check.js`). Tre numeri, due valori, nella
stessa delivery. Correzione: allineare il backlog a 14, o a 11 se si applica il serio 3.

**[MINORE] `e2e/scripts/close-gate-check.js:350` — il messaggio di fallimento del check sui backup indica una directory che nessuno guarda.**
`backupsUnder` è stato corretto a `com.sublore.app` (`:222`), ma il dettaglio stampa ancora
`path.join(saveHome, "sublore", "backups")`. Quando quel check fallisce, chi legge va a guardare un
percorso che non c'entra. Correzione: costruire il percorso una volta sola e riusarlo nei due punti.

**[MINORE] `e2e/scripts/close-gate-check.js:324` — il ramo Salva non controlla i superstiti, il ramo Scarta sì.**
A `:286` il valore di ritorno di `reap` viene usato per il check «no process survived discard». A `:324` la
stessa chiamata viene fatta e il valore buttato via. Salva chiude l'app esattamente allo stesso modo, ed è
il ramo in cui l'app ha appena scritto su disco: se qualcosa sopravvive lì, conta di più. Correzione:
riusare il ritorno e aggiungere il check simmetrico, alzando `EXPECTED_CHECKS`.

**[MINORE] `e2e/scripts/close-gate-check.js:174, 183, 187, 192` — attese fisse residue, tre delle quali non giustificate.**
`:174` (2000 ms, attesa che la webview dipinga) è documentata sul posto e giustificata: da lì non c'è DOM
su cui attendere. `:183` (1500 ms dopo il click su Open), `:187` (600 ms dopo il doppio click) e `:192`
(2500 ms dopo Return) non hanno né giustificazione né condizione. Nessuna delle tre produce un falso verde:
se scadono corte il documento resta pulito e il test fallisce. Ma falliscono tutte con il messaggio del
serio 4, che dice al lettore di ignorare la corsa. Correzione: sostituirle con un'attesa su condizione (la
riga della cue che mostra `SUBLORE_N1`, oppure il marcatore dirty), o quantomeno scrivere accanto perché
quel numero.

**[MINORE] `e2e/scripts/close-gate-check.js:267-271` — il check «cancel left the app running» è racy.**
`state.exit === null` viene letto subito dopo che il dialogo è sparito. Se Escape avesse chiuso l'app,
`exit` potrebbe non essere ancora popolato al momento della lettura, e il check passerebbe per il motivo
sbagliato. Il test nel suo insieme non ne resta ingannato: il `waitForDialog` successivo (`:279`) dichiara
esplicitamente «the app exited instead of asking». Ma il check non prova quello che il suo nome dice.
Correzione: riasserirlo dopo il secondo `requestClose`, o attendere brevemente prima di leggerlo.

---

## Punti che ho guardato e che sono a posto

- **`save_locked` e l'assenza di race di revisione** (`mod.rs:387-396`). Un solo lock per percorso.
  Nessuna finestra fra `check_revision` e la scrittura. Nessun rilievo.
- **Dialoghi da thread non-main** (`lib.rs:226-232`, `:271-278`). Sicuri.
  `tauri-plugin-dialog-2.7.2/src/desktop.rs:222` fa `handle.run_on_main_thread(...)` internamente prima di
  costruire il dialogo, e blocca sul future in un thread separato (`:225-226`). Chiamare `.show()` dal thread
  del callback del primo dialogo, o dall'interno della closure già sul main thread in `close_window`, in
  entrambi i casi finisce per accodare sul main loop senza bloccarlo. Nessun deadlock del tipo che
  `project::choose_path` documenta. Nessun rilievo.
- **Il test sul poison prova quello che dice** (`src-tauri/tests/subtitle_editing.rs`,
  `save_current_still_saves_through_a_poisoned_lock`). Il thread che avvelena rilascia la guardia
  nell'unwind, quindi il mutex è sbloccato ma avvelenato; il test lo verifica esplicitamente con
  `assert!(slot.is_poisoned())` prima di chiamare `save_current`. Non è un lock che «si sblocca da solo»
  mascherato da recupero: il recupero dal poison è davvero l'unica cosa sotto test. Passa.
  Unica riserva già coperta dal serio 2: il test non verifica cosa succede alla sessione _dopo_.
- **§7, budget prestazioni.** `unsaved_work` gira una volta per richiesta di chiusura, non in un loop, e usa
  `try_lock`. Nessun percorso nuovo blocca il main loop: `show_with_result` ritorna subito, `save_current`
  e `close_session` girano sul thread del callback, `window.destroy()` è marshallato via
  `run_on_main_thread`. Nessun budget toccato. Nessun rilievo.
- **§6, `unwrap()` fuori dai test.** Nessuno nel codice nuovo. `cargo clippy -- -D warnings` pulito.
- **§9, stringhe i18n-ready.** Tutte le stringhe nuove stanno in `src-tauri/src/strings.rs`, che è il punto
  di raccolta già usato da `crash_body` e dai titoli dei file dialog. Convenzione rispettata.
- **Teardown prima della distruzione** (`lib.rs:254-257`). `asr::shutdown` e `shutdown_video` girano prima
  di sapere se `destroy()` riuscirà, quindi un fallimento lascia una finestra aperta e mezza morta. È una
  scelta consapevole: il doc comment a `:245-250` la dichiara e `strings.rs:33-38` la dice all'utente.
  Non un difetto nascosto.
- **`shutdown_project` non è nel percorso del gate.** Corretto: `close_window` rispecchia il vecchio ramo
  `CloseRequested`, e `shutdown_project` continua a girare su `Exit` (`lib.rs:108-113`), che `destroy()`
  fa comunque scattare.
- **Il "Known gap" dichiarato in BACKLOG** (editor inline con testo non committato → sessione pulita →
  chiusura senza domanda). Verificato coerente con il codice, ed è la ragione per cui `openAndDirty`
  preme Return (`close-gate-check.js:191`). Dichiararlo invece di nasconderlo è la cosa giusta e va detto.

## Nota di processo

`docs/reports/` esiste ed è vuota. `docs/reviews/review-prompt.md:15` stabilisce che una review il cui
file di rapporto manca è una review fallita: il rapporto della prima passata, quello che questa delivery
cita come fonte di tre bloccanti, non risulta committato da nessuna parte. Questo file è il primo in quella
directory. Vale la pena recuperare il primo, o dire esplicitamente che è andato perso.

---

## VERDETTO

**RICHIEDI MODIFICHE.**

Bloccante da correggere prima del merge: `save_current` che scrive su un file non sporco, e che sovrascrive
modifiche esterne senza controllarle (§3). È una riga.

Seri da correggere o da acknowledgare esplicitamente nella descrizione della delivery: `.parent()` no-op con
un commento che promette il contrario, il poison che non viene azzerato con un messaggio d'errore che manda
l'utente su una strada morta, le tre asserzioni vuote contate dal guard che dovrebbe impedirle, il messaggio
diagnostico che incolpa il setup per una possibile regressione del gate, e le modifiche non committate che
lascerebbero la CI rossa al primo run dopo il merge.

Il resto del cambiamento è solido: la giustificazione del recupero dal poison è vera e l'ho verificata nei
sorgenti di `sublore-edit`, `save_locked` elimina davvero la doppia presa del lock, l'assunzione più
rischiosa del disegno (il rimapping dei bottoni custom del plugin) l'ho verificata in
`tauri-plugin-dialog` e `rfd` e regge, e `shutdown-check.js` chiude davvero il secondo criterio di
accettazione. Cosa che non ho potuto verificare, e che dichiaro come non verificata (§9): la suite e2e non
è stata eseguita in questa passata, perché non c'è display e il binario non è costruito.
