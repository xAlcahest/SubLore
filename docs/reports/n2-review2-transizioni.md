# N2 — seconda passata: cosa hanno rotto le correzioni

Revisione della diffe non committata su `main`, 2026-08-30. Lente imposta da WORKFLOW.md §4b: le
correzioni della prima passata sono codice nuovo scritto sotto pressione di review, e questa passata
cerca esplicitamente cosa hanno rotto. Priorità del proprietario: lo stato di visibilità esplicito in
`src-tauri/src/video/mod.rs`, che è il cuore della superficie.

Superficie esaminata: `src-tauri/src/video/mod.rs`, `e2e/lib/pixels.js` (nuovo), `e2e/lib/env.js`
(nuovo), `e2e/specs/video-surface.spec.js` (nuovo), `e2e/wdio.conf.js`, `e2e/README.md`,
`e2e/scripts/{shutdown,close-gate}-check.js`, `docs/design/shell-layout.md`.
Contorno letto per tracciare le transizioni: `video/surface/{mod,linux,windows}.rs`,
`video/player.rs`, `crash/force.rs`, `lib.rs`, `main.rs`, `src/App.tsx`,
`src/components/{VideoStage,VideoControls,VideoOpenBar}.tsx`, `src/hooks/useVideoPlayer.ts`,
`e2e/lib/{proc,x11,paths}.js`, `e2e/specs/video.spec.js`, `.github/workflows/ci.yml`, più i due
rapporti della prima passata e `docs/reports/n2-probe.md`.

## Come sono state ottenute le prove, e cosa non ho misurato

Tutti i rilievi qui sotto vengono da lettura del codice e dal tracciamento delle transizioni, non da
esecuzione. **Non ho compilato, non ho lanciato la suite E2E, non ho misurato pixel, non ho toccato
Windows.** Dove cito un numero è ripreso dai rapporti della prima passata o da `n2-probe.md` e lo
dico. Dove una conclusione dipende da un comportamento che nessuno in questo repo ha osservato — il
caso "mpv carica mentre la superficie è smappata" — lo scrivo dentro il rilievo invece di
presentarlo come fatto (CLAUDE.md §9).

---

## Bloccanti

### [BLOCCANTE] src-tauri/src/video/mod.rs:138-144 — una seconda apertura che fallisce spegne la visibilità dell'apertura ancora in corso, e da lì non si torna più indietro

Il difetto: `VIDEO_OPEN` è un booleano senza proprietario, quindi il ramo d'errore di una
`video_open` azzera lo stato di un'altra `video_open` che sta riuscendo, e dopo la correzione
nessun aggiornamento di regione può più rimediare.

Scenario concreto, raggiungibile con due clic. `VideoOpenBar` disabilita il bottone su
`busy = state.status === "loading"` (`App.tsx:49`), e quello stato arriva dal backend via evento
`video://state`: fra il primo submit e l'arrivo dell'evento il bottone è ancora attivo, quindi un
doppio clic (o due Invio) manda due `video_open`. Traccia:

1. A: `VIDEO_OPEN=true`, `set_shown(true)` mappa la superficie, poi `player.open` parte e blocca.
2. B: `VIDEO_OPEN=true`, `set_shown(true)` è un no-op, `player.open` ritorna subito
   `Err("another open is already in progress")` (`player.rs:245-249`) — il player quel caso lo
   prevede per costruzione.
3. B entra in `opened.is_err()` (riga 138): `VIDEO_OPEN=false` e `set_shown(false)`, **smappa la
   superficie mentre A sta caricando**.
4. A finisce bene: stato `ready`, audio in riproduzione, nessun fotogramma.
5. La banda `.app__error` di B compare, il layout si muove, parte una `video_set_region` con
   regione reale: `apply_region` (riga 230) legge `VIDEO_OPEN=false` e **lascia la superficie
   nascosta**. Ogni resize successivo fa lo stesso.

Risultato: video aperto, audio che va, schermo vuoto, e l'unico modo di uscirne è riaprire il file.
Prima di questa correzione il ramo `else` di `apply_region` non mostrava nulla, quindi lo stato
finale era lo stesso; la differenza è che N2 esiste proprio per rendere il ri-show possibile, e
questo cammino lo disabilita per il resto della sessione senza dirlo a nessuno. È il difetto che la
consegna dichiara di chiudere, lasciato aperto da una porta laterale.

Peggioramento possibile, che segnalo come domanda aperta e non come fatto: se il passo 3 cade mentre
mpv sta costruendo la sua uscita, il commento di `surface/mod.rs:85-86` sostiene che la finestra
figlia di mpv nasce smappata e non torna. Nessuno lo ha misurato (la sonda ha coperto solo
hide/show a mpv già agganciato), quindi non lo do per certo.

Correzione: la visibilità non può essere un booleano che chiunque può azzerare. O `video_open` porta
un numero di generazione e solo l'apertura più recente ha diritto di scriverlo, oppure — meglio, e
più corto — la visibilità si deriva dallo stato del player (`PlayerStatus::Ready`) invece di essere
mantenuta a mano in un secondo posto che deve restare d'accordo con il primo. In entrambi i casi il
caso va coperto da un test: due `video_open` in volo, la seconda respinta, la prima resta visibile.

### [BLOCCANTE] e2e/specs/video-surface.spec.js — nessun test lega le due correzioni della prima passata, ed erano state chieste per nome

Il difetto: i due difetti corretti nel backend — superficie mappata all'avvio senza video, e lastra
vuota che ritorna al primo resize dopo un'apertura fallita — non hanno alcuna copertura
comportamentale, quindi la prossima modifica a `apply_region` li reintroduce in silenzio.

Scenario concreto: qualcuno semplifica `apply_region` togliendo la consultazione di `VIDEO_OPEN`
(riga 230) — è esattamente la forma che il codice aveva ventiquattro ore fa. `cargo test`,
`pnpm lint` e i trenta test E2E restano verdi: nessuna asserzione in tutto il repo guarda la
superficie prima che un video sia aperto. `grep -n "IsUnMapped" e2e/specs/` dà solo le tre
transizioni interne alla nuova spec, tutte dopo `video_open`. `video.spec.js:126,136` asserisce
`IsViewable` **dopo** l'apertura, quindi non distingue i due mondi.

Non è una svista di questa passata: entrambi i rapporti della prima l'hanno chiesto esplicitamente
(`n2-review-test.md`, bloccante 3: "un caso E2E che all'avvio, senza aprire niente, verifica che la
superficie sia `IsUnMapped` mentre `.stage__empty` è visibile"; `n2-review-codice.md`, bloccante 1 e
serio 2). Il codice è stato corretto, il test chiesto no. CLAUDE.md §5.3 vuole una fixture di
regressione per ogni difetto corretto, e §5.2 dice che i test comportamentali sono lo strato
primario: una consegna che è per metà una consegna di test non può chiudere due bloccanti senza
lasciarne uno legato.

Il costo è basso e i pezzi ci sono già. Prima di aprire qualunque video, nella stessa spec:
la superficie esiste (`childWindows`), è `IsUnMapped`, `.stage__empty` è visibile e la saturazione
del rettangolo dello stage è sotto `PICTURE`. Secondo caso: aprire un percorso inesistente,
attendere `.app__error`, forzare un resize (o un collasso/ripristino dello stage, che l'harness sa
già fare) e pretendere che la superficie resti `IsUnMapped`. Sono i due scenari che i rapporti
hanno descritto a parole; qui diventano asserzioni.

---

## Seri

### [SERIO] src-tauri/src/video/mod.rs:125-136 — le due uscite anticipate con `?` lasciano `VIDEO_OPEN=true` e la superficie mappata senza alcun video

Il difetto: il rollback esiste solo nel ramo `opened.is_err()` (riga 138), ma fra l'accensione del
flag (riga 128) e quel controllo ci sono due `?` che escono dalla funzione senza spegnere nulla.

Scenario concreto, quello del timeout, ed è il peggiore perché lo stato viene scritto **dopo** che
il chiamante ha già rinunciato. `on_main_thread` (righe 180-194) accoda la chiusura e aspetta
`MAIN_THREAD_TIMEOUT` = 2 s. Se il thread principale è occupato più a lungo, `recv_timeout` scade,
riga 131 esce con `Err("the main thread did not answer")`, l'utente vede la banda d'errore e nessun
video. Poi il thread principale si libera ed **esegue comunque la chiusura**: `VIDEO_OPEN=true` e
`set_shown(true)`. Da quel momento c'è una lastra opaca sopra lo stage, nessun video dietro, e ogni
`video_set_region` successiva la tiene mappata perché il flag dice che un video è aperto. È
letteralmente il sintomo del bloccante 1 della prima passata, reso permanente dal flag invece che
transitorio.

Secondo cammino, più raro: `spawn_blocking(...).await` a riga 134-136 ritorna `Err` su `JoinError`
(panic dentro `player.open`, o runtime in chiusura). Il `?` a riga 136 esce prima del controllo
`opened.is_err()`, con lo stesso esito.

Non lo classifico bloccante perché l'innesco è uno stallo dell'infrastruttura e non un gesto
ordinario, ma non ha alcuna via di recupero.

Correzione: un solo punto di uscita per il percorso di apertura. Il rollback va eseguito su
qualunque uscita non riuscita, non solo su `opened.is_err()`: incapsulare il corpo in una funzione
interna e applicare la compensazione sul suo `Err`. In più, non scrivere lo stato dentro una
chiusura che può girare dopo la scadenza: il flag va acceso dal chiamante quando la chiusura ha
davvero risposto, oppure la chiusura deve controllare che il canale sia ancora vivo prima di
lasciare effetti dietro di sé.

### [SERIO] src-tauri/src/video/mod.rs:125-129 — lo stato è esplicito solo a metà: `video_open` mostra senza guardare la regione, e dopo l'apertura nessuno ri-deriva la visibilità

Il difetto: `apply_region` consulta `VIDEO_OPEN`, ma `video_open` non consulta la regione; e la
"regione vuota" non è memorizzata da nessuna parte, quindi la superficie viene mappata con la
geometria vecchia.

Scenario concreto, ed è quello che la decisione 1 spedita in questa stessa diffe descrive: un
livello HTML è aperto, la shell ha mandato una regione vuota, `apply_region` è uscito a riga 227
**senza chiamare `set_region`** — quindi il rettangolo del server X è ancora quello di prima. Se
mentre il livello è aperto parte una `video_open` (voce di menu "apri video", riapertura da progetto,
o semplicemente lo stage collassato), la riga 129 mappa la superficie sulla geometria vecchia, cioè
sopra il livello. E non c'è nulla che ri-derivi la visibilità a fine caricamento: la superficie
resta lì finché non arriva un aggiornamento di regione, che `shell-layout.md:163` dice esplicitamente
che la shell **non manderà** mentre un livello è aperto.

`shell-layout.md:149` chiama questa `show()` "a belt, not the design" e conta sul fatto che il
frontend chiuda il proprio livello prima di spedire. Un invariante mantenuto da una convenzione di
frontend, con il backend che ha ora lo stato per farne a meno, è precisamente ciò che la prima
passata chiedeva di eliminare.

Nota onesta sul rovescio: il caso opposto — rifiutarsi di mostrare quando la regione è vuota — apre
la domanda su cosa faccia mpv se costruisce la sua uscita con il genitore smappato.
`surface/mod.rs:85-86` sostiene che resti smappata; nessuno lo ha misurato. Se è vero, "aprire un
video mentre un livello è aperto" non ha nessuna risposta buona nel codice attuale e la decisione
spetta al proprietario.

Correzione: memorizzare l'ultima regione accanto ai due flag, mostrare solo se `VIDEO_OPEN &&
!region.is_empty()`, e ri-asserire la visibilità alla fine di un'apertura riuscita invece di
affidarla al solo momento pre-caricamento.

### [SERIO] e2e/specs/video-surface.spec.js:176-179 — l'attesa che dovrebbe provare "l'orologio è fermo" è soddisfatta dal suo primo campione

Il difetto: `waitFor` ritorna al primo valore vero (`proc.js:18-21`), quindi
`waitFor(async () => ((await transport()) === frozen ? true : null), { timeout: 5000 })` esce
immediatamente, perché `frozen` è stato letto la riga prima. Non prova nulla; il commento sopra dice
"mpv's own clock, held still. This is the proof the video is stopped, not the button."

Scenario concreto: il clic su Pause raggiunge il bottone (l'etichetta va a "Play", e quella parte è
una prova vera, aggiunta bene in questa correzione) ma mpv resta in riproduzione — per esempio
`video_pause` fallisce e finisce solo nella banda d'errore, oppure la scrittura ottimistica di
`useVideoPlayer.ts:96` corregge l'etichetta prima che la property `pause` di mpv sia cambiata. Il
"wait" a riga 176 passa comunque al primo giro. Resta in piedi solo l'asserzione finale di riga 196,
un campione singolo su un'etichetta a risoluzione di un secondo (`VideoControls.tsx:6-11`): una
ripartenza che dura meno del confine del secondo passa verde.

È la stessa classe di difetto che la prima passata ha trovato su questo stesso test ("confronta
l'etichetta con sé stessa"), sostituita da una costruzione diversa che ha lo stesso vizio.

Correzione: la prova che qualcosa **non** avviene non si scrive con `waitFor`. Leggere l'orologio,
dormire un intervallo dichiarato (≥ 2 s, così il confine del secondo è attraversato), rileggere e
asserire l'uguaglianza; oppure pretendere che l'attesa "l'orologio avanza" vada in timeout, che è la
stessa cosa detta col verso giusto. E lo stesso vale prima del nascondimento, dove la stessa
costruzione vacua compare per stabilire la precondizione.

### [SERIO] docs/design/shell-layout.md:141,151,163 — il documento di design spedito con la correzione descrive un backend che la correzione ha appena eliminato

Il difetto: tre affermazioni su cui M2.0 è progettata sono ora false o obsolete, e la prima passata
aveva già chiesto di riscriverne due.

- Riga 151: "`set_region` never maps a hidden window, so sending a rectangle as the way back would
  move an unmapped window and show nothing". Dopo la correzione, con un video aperto, mandare un
  rettangolo reale **mappa** la superficie (riga 230 di `mod.rs`). La frase è il contrario di ciò che
  il codice fa, ed è la premessa che porta M2.0 a progettare un ritorno in due passi.
- Riga 163: "When the set empties it sends that rectangle and then shows, in that order". Non esiste
  alcun comando "show": l'interfaccia IPC ha cinque comandi (`open`, `play`, `pause`, `seek`,
  `set_region`) e dopo questa correzione il rettangolo da solo basta. M2.0 è pianificata su un passo
  che non esiste e non serve.
- Riga 141: "any region update while a layer is open **raises** the surface again" — ora la mostra,
  non solo la rialza, il che rende la regola "non mandare regioni mentre un livello è aperto"
  portante invece che prudenziale.
- Riga 149 cita `video/mod.rs:106`, che dopo la correzione non è più quella riga.

Scenario concreto: chi implementa M2.0 legge il documento, costruisce il ritorno in due passi, non
trova il comando "show", e o lo inventa (secondo percorso di visibilità, che il documento stesso
vieta) o conclude che il documento è vecchio e riprogetta da solo la parte che il proprietario aveva
già deciso.

Correzione: riscrivere le tre frasi sul comportamento attuale nello stesso commit che lo introduce, e
aggiornare i riferimenti di riga. CLAUDE.md §9: un documento che descrive codice inesistente è una
dichiarazione non verificata presentata come verificata.

### [SERIO] e2e/lib/env.js:6-13 e e2e/wdio.conf.js:20-23 — la diagnosi scritta nel commento resta sbagliata, e la consegna dichiara che tutti i rilievi sono stati corretti

Il difetto: il commento attribuisce a GTK una scelta che GTK non può fare.
`src-tauri/src/main.rs:6-9` imposta `GDK_BACKEND=x11` come prima istruzione di `main`, prima di
`sublore_lib::run()` e quindi prima di `gtk_init`, con il commento che dice perché. Il componente
che può ignorare `--wid` in presenza di `WAYLAND_DISPLAY` è libmpv, non GTK. Il rilievo era già stato
scritto nella prima passata (`n2-review-codice.md`, serio 3, con il ragionamento completo) e non è
stato applicato; il secondo rapporto della prima passata affermava il contrario ("la diagnosi è
giusta") senza confrontarla con `main.rs`.

Scenario concreto, che è il motivo per cui non è pedanteria: la sessione del proprietario è Wayland.
L'app forza GTK su XWayland e la superficie X11 nasce bene, ma `WAYLAND_DISPLAY` resta
nell'ambiente del processo e libmpv vede le stesse condizioni che hanno bruciato due esecuzioni della
sonda. La difesa è stata messa **solo nell'harness** (`delete env.WAYLAND_DISPLAY`), il che rende il
caso irriproducibile dalla suite: se il video non si vede sul desktop reale del proprietario, nessun
test lo dirà mai. Non l'ho verificato — richiede lanciare l'app sul suo compositore vivo.

Correzione: riscrivere il commento attribuendo il comportamento a libmpv; portare la difesa nel
prodotto (togliere `WAYLAND_DISPLAY` dall'ambiente accanto a `main.rs:9`, oppure fissare
`gpu-context` in `PlayerConfig::embedded`); e una singola esecuzione
`env -u GDK_BACKEND ./target/debug/sublore` su sessione Wayland per sapere se il difetto è vivo.
Finché il commento resta com'è, il prossimo che passa cancella la riga giusta credendola ridondante —
e `GDK_BACKEND: "x11"` in `env.js:16` **è** davvero ridondante, perché il binario se lo imposta da sé.

### [SERIO] src-tauri/src/video/mod.rs:24-36 — tre stati che devono restare d'accordo, in tre `thread_local!` separati, e due falliscono in silenzio

Il difetto: `SURFACE` fallisce rumorosamente se letta dal thread sbagliato o dopo la distruzione
("the video surface is gone", riga 204); `VIDEO_OPEN` e `SHOWN` no. Un `Cell` thread-local letto da
un altro thread restituisce il proprio valore iniziale, cioè `false`, senza errore, senza log, senza
compilazione fallita.

Scenario concreto, e non è ipotetico: M2.0 (decisione 1, progettata in questa stessa diffe) ha
bisogno di un comando di visibilità. Chi lo scrive ha davanti `set_shown`, una funzione privata che
si documenta "main thread only" in un commento, e `VIDEO_OPEN.with(Cell::get)`, che compila e gira
ovunque. Se lo legge fuori da `run_on_main_thread` ottiene `false` e nasconde un video aperto, senza
alcun sintomo diagnosticabile: nessun errore risale all'UI, il flag "corretto" resta intatto su un
altro thread.

Sulla domanda posta su `take_surface`/`shutdown`: i due flag **restano true** dopo che la superficie è
stata distrutta (riga 105-111 non li tocca). Oggi non morde, e lo escludo come difetto vivo dopo
averlo tracciato: `shutdown_video` viene chiamato solo su `CloseRequested` senza lavoro non salvato e
su `ExitRequested`/`Exit` (`lib.rs:104,111`), mentre il percorso del close gate con "annulla" non lo
chiama affatto, quindi non esiste una sessione che continui con la superficie distrutta. Ma
l'invariante è mantenuto per coincidenza, non per costruzione: il giorno in cui la superficie viene
ricreata (cambio schermo, riaggancio della finestra), `SHOWN=true` fa uscire `set_shown` a riga 41
senza mostrare nulla, e la nuova superficie resta invisibile per sempre.

Correzione: i tre stati sono un solo stato. Metterli dentro l'unico `thread_local!` che già esiste —
una struct nello slot di `SURFACE`, con la visibilità e l'apertura come campi — rende impossibile
leggerli senza la superficie, li azzera gratis in `take_surface`, e fa fallire con lo stesso
messaggio esistente qualunque accesso dal thread sbagliato.

---

## Minori

### [MINORE] e2e/specs/video-surface.spec.js:104-108 — il controllo su `.app__error` è una lettura sola subito dopo il clic, quindi non può quasi mai scattare

La banda d'errore compare dopo il giro completo IPC + `player.open`, cioè decine o centinaia di
millisecondi dopo; `browser.execute` viene eseguito subito dopo `clickAt`. Scenario: il percorso non
si apre, il controllo legge `null`, e il fallimento arriva trenta secondi più tardi con "the surface
with mpv attached inside it", accusando mpv per un problema di apertura. È il rilievo serio della
prima passata (`n2-review-test.md`) chiuso con una guardia che non guarda. Correzione: `waitFor` sullo
stato pronto (`.stage__empty === null` e `.controls__button` abilitato) **oppure** su `.app__error`,
e fallire con il messaggio dell'app.

### [MINORE] e2e/specs/video-surface.spec.js:88-89 — `centreOf` ancora duplicata da `video.spec.js` e la guardia sul null è ancora persa

`const field = await centreOf(".bar__input")` seguito da `field.x`: se l'elemento manca il test muore
con `TypeError: Cannot read properties of null` dentro `before`, senza dire quale selettore mancava.
`video.spec.js` incapsula la stessa funzione in `clickElement`, che il null lo controlla. Rilievo
della prima passata, non applicato. Correzione: sollevare `centreOf`/`clickElement` in `e2e/lib/`.

### [MINORE] e2e/specs/video-surface.spec.js:57-63 vs :216 — `surfaceWindow` tollera più figli grandi, il test 3 asserisce che ce n'è esattamente uno

O ne può esistere più d'uno, e allora `toHaveLength(1)` è fragile, o non può, e allora il filtro più
l'ordinamento per area sono complessità morta che nasconde il criterio vero. `video.spec.js:116-134`
identifica la superficie facendo combaciare la geometria con il rettangolo di `.stage__surface` entro
2 px, che è il criterio giusto e già scritto. Rilievo della prima passata, non applicato.

### [MINORE] e2e/specs/video-surface.spec.js:113-119, 120, 145, 190, 217 — la geometria della superficie è congelata al `before`

`surface` viene catturata una volta e le sue coordinate assolute alimentano ogni `saturation()`. Se il
layout si sposta durante la corsa (la banda d'errore che compare, per esempio) il ritaglio misura
l'area sbagliata. Rilievo della prima passata, non applicato. Correzione: rileggere la geometria prima
di ogni misura, o asserire che non sia cambiata.

### [MINORE] e2e/specs/video-surface.spec.js:45-54 — nessun `afterEach` che ripristini lo stage

`setStageCollapsed(true)` scrive `style.height = "0px"` nel DOM e nessuno lo rimette a posto in caso
di fallimento a metà. Scenario: il test 1 fallisce dopo il collasso; il test 2 parte con lo stage già
collassato, aspetta "the surface to hide" su una superficie già nascosta e scade con un messaggio che
non c'entra. Un fallimento ne diventa tre. Rilievo della prima passata, non applicato.

### [MINORE] e2e/specs/video-surface.spec.js:139,182,202 — lo stato nascosto non viene mai misurato sui pixel

Le tre spegnimenti asseriscono `IsUnMapped` e mai che la saturazione sia **sotto** `PICTURE`. La
separazione fra i due stati regge oggi per un'invariante accidentale (sotto la superficie c'è il nero
uniforme di `.stage`); il giorno che qualcuno mette un poster o un gradiente sotto lo stage, la soglia
viene superata senza che mpv disegni niente. Rilievo della prima passata, non applicato. Nello stesso
gruppo: il test 2 non ricontrolla `mapState` dopo il ri-show, e il ciclo dei dieci non misura pixel in
mezzo, quindi una degradazione progressiva del ri-show passa tutti e dieci i giri.

### [MINORE] src-tauri/src/video/mod.rs:29-31 e 223-225 — commenti di tre righe che rinarrano il difetto

La policy sui commenti (CLAUDE.md §6 e la regola del proprietario) concede 1-2 righe per blocco e
manda la narrazione del bug nella descrizione della consegna. Qui ci sono due commenti da tre righe
che raccontano cosa faceva il codice sbagliato ("put an opaque slab over the empty stage at
startup", "without ever showing a slab over an empty stage"). La prima passata aveva già mosso lo
stesso rilievo sulla versione precedente dello stesso commento.

### [MINORE] src-tauri/src/video/mod.rs:139 — `let _ = on_main_thread(...)` ingoia il fallimento della compensazione

Se il dispatch al thread principale fallisce, la superficie resta mappata con `VIDEO_OPEN=true` e
nessuno lo saprà mai: CLAUDE.md §6 vieta gli errori ingoiati in silenzio. Preesiste alla correzione
come forma, ma dopo la correzione la conseguenza è uno stato incoerente e non solo una finestra
mappata. Correzione: almeno un log, o propagare l'errore accanto a quello dell'apertura.

### [MINORE] e2e/lib/pixels.js:63-71 — `run.status` non viene controllato e lo stderr di ffmpeg viene buttato via

Se ffmpeg esce diverso da zero (regione parzialmente fuori schermo, dimensioni degeneri, x11grab che
non riesce ad aprire il display), l'unico messaggio è "ffmpeg printed no signalstats saturation for
{...}", con la causa vera — che ffmpeg ha stampato in chiaro — scartata. Nella stessa funzione: `rect`
viene usato senza validare che larghezza e altezza siano interi positivi, quindi un rettangolo
degenere produce lo stesso messaggio cieco. Correzione: controllare `status` e riportare la coda di
`stderr` nel messaggio.

### [MINORE] e2e/lib/pixels.js:8-10 — "ffmpeg is already a harness dependency on both platforms" è un'affermazione che nessuno può verificare

`x11grab` esiste solo su Linux e non esiste alcun job E2E su Windows (`ci.yml` ha solo
`e2e smoke (ubuntu)`), quindi il modulo è per costruzione Linux-only e la frase sulle due
piattaforme non è verificata da niente. CLAUDE.md §9 e §5.5. Nella stessa voce: `requireFfmpeg()`
viene chiamata nel `before` della spec e non in `onPrepare` accanto a `requireDisplay`,
`requireAppBinary` e `requireVideoFixture`, che è la convenzione che il rapporto della prima passata
citava; il prerequisito manca quindi dopo che l'app è già stata lanciata.

### [MINORE] e2e/README.md:7-11 — "Each tool is checked with its own message" è vero per uno strumento su quattro

Il paragrafo nuovo elenca `xdotool`, `xwininfo`, `python3` con python-xlib e `ffmpeg`, e dichiara che
ognuno è controllato con un messaggio proprio. In `e2e/lib/paths.js` esistono `requireAppBinary`,
`requireVideoFixture`, `requireCloseWindowTool` e `requireDisplay`: nessun controllo per i primi tre
strumenti. Correzione: o si aggiungono i controlli, o la frase dice quello che c'è.

### [MINORE] e2e/lib/env.js vs e2e/wdio.conf.js:20-23 — la stessa regola scritta due volte

Rilievo della prima passata, non applicato: `appEnv()` serve chi fa `spawn`, `wdio.conf.js` muta
`process.env` perché tauri-driver eredita. Due copie della stessa regola, e la seconda che dimenticherà
di seguire la prima non fallirà, ripresenterà solo il sintomo che alla sonda è costato due esecuzioni.

### [MINORE] git status — la consegna mescola ancora N2 con i documenti M2.0

`docs/design/m2-0-tasks.md` e i due `m2-0-critique-*.md` (2.022 righe) viaggiano insieme a N2.
WORKFLOW.md §4 chiede consegne grandi quanto una sola sessione di revisione. Rilievo della prima
passata, non applicato.

### [MINORE] src-tauri/src/video/surface/mod.rs:85-86 — la correzione al commento chiesta dalla sonda non è mai stata fatta

`n2-probe.md:38` aveva chiesto una sola cosa al codice: che il commento smettesse di presentare la
finestra figlia di mpv come sempre presente, perché è il segnale onesto dell'aggancio e lo stato di
mappa da solo non lo è. Il commento è invariato, e nel frattempo `video-surface.spec.js:111-112` ha
imparato la lezione giusta e la scrive nel proprio commento. Il codice e il test dicono due cose
diverse sullo stesso fatto.

---

## Punti guardati e chiusi senza rilievo

- **I punti di crash forzato non corrompono lo stato.** `trip(ForcePoint::Open)` sta a riga 120,
  prima di qualunque scrittura; `trip(ForcePoint::MainThread)` sta a riga 127, prima di
  `VIDEO_OPEN.set(true)`. Un panic in entrambi i casi lascia i due flag come li ha trovati, e il
  chiamante riceve "the main thread did not answer" perché il sender viene distrutto senza inviare.
  Verificato leggendo `crash/force.rs` (il `trip` fa `panic!`, e in release non esiste proprio) e
  l'ordinamento delle righe.
- **Riapertura dopo un errore.** File rotto → `VIDEO_OPEN=false` + hide; poi file buono →
  `VIDEO_OPEN=true` + show. Tracciato riga per riga: lo stato regge, e `SHOWN` segue perché ogni
  transizione passa da `set_shown`. Nessun rilievo, a patto che le due aperture non si sovrappongano
  (bloccante 1).
- **`SHOWN` parte da false ed è vero che la superficie nasce non mappata.** Su Linux
  `gdk::Window::new` crea una finestra non mappata e né `ensure_native` né `set_pass_through` la
  mappano (`surface/linux.rs:27-44`); su Windows `WS_CHILD` senza `WS_VISIBLE` non è visibile
  (`surface/windows.rs:34`). L'unico attore che potrebbe mappare dall'esterno è mpv, che mappa la
  propria figlia e non il genitore, e comunque entra in scena dopo che `video_open` ha già portato
  `SHOWN` a true. Non esiste quindi il cammino "mappata ma il flag dice di no".
- **La deduplicazione di `set_shown` non perde il `raise`.** `set_region` fa `move_resize` + `raise`
  a ogni aggiornamento (`linux.rs:65-66`), indipendentemente da `SHOWN`, quindi la superficie resta
  sopra la webview anche quando `show()` non viene riemessa. Il doppio `raise` per aggiornamento
  segnalato dalla prima passata è effettivamente sparito.
- **Il close gate non distrugge la superficie.** `shutdown_video` non viene chiamato quando il gate si
  alza (`lib.rs:95-106`): con lavoro non salvato il ramo è `prevent_close` e basta. Quindi non esiste
  oggi una sessione che continui a girare dopo `take_surface`, ed è la ragione per cui i flag rimasti
  true dopo la distruzione sono un rischio latente e non un difetto vivo.
- **`EXPECTED_TESTS = 30` è esatto.** Contate le `it(` per file: asr 5, editor 10, project 5,
  subtitle 3, title 2, video 2, video-surface 3. La guardia resta un `<`, quindi continua a fallire
  su una spec saltata.
- **Il bloccante CI della prima passata è chiuso davvero.** `ffmpeg` è già nella lista `apt-get` di
  entrambi i job (`ci.yml:34` e `:138`) e non è stato aggiunto da questa diffe: la sostituzione di
  ImageMagick con ffmpeg non introduce alcuna dipendenza non dichiarata sul runner. Nessun residuo di
  `magick`/`import` nell'harness.
- **`video.spec.js:126,136` torna a significare qualcosa.** Con la superficie di nuovo non mappata
  finché un video non è aperto, l'asserzione `IsViewable` dopo l'apertura può di nuovo fallire per il
  motivo per cui era stata scritta. L'indebolimento silenzioso segnalato dalla prima passata è
  rientrato.
- **CLAUDE.md §3, sicurezza dati.** Nessun punto della diffe scrive, sposta o rinomina file
  dell'utente. `pixels.js` legge lo schermo e scrive su `/dev/null` (`-f null -`), e la directory
  temporanea di screenshot della versione precedente è sparita del tutto. Nessuna scrittura atomica,
  nessun backup, nessun database coinvolti.
- **Nessun `unwrap()` fuori dai test, nessun `any`, nessuna chiave o codice dei moduli chiusi.**
  Verificato sulle righe aggiunte.
- **Il test 1 ("with the video playing") ora prova la propria precondizione.** L'orologio del
  trasporto viene letto prima e si aspetta che cambi (righe 132-136), e la stessa prova viene ripetuta
  dopo il ri-show (righe 152-156), che è la metà di AC che la prima passata segnalava come scoperta.
  Questa correzione è buona e regge: `.controls__time` viene da `video://position`, cioè da `time-pos`
  di mpv (`player.rs:490-500`), non da stato React. Il difetto resta solo sul caso in pausa, sopra.

---

## Verdetto

**RICHIEDI MODIFICHE.**

Le correzioni della prima passata hanno chiuso i difetti giusti — la superficie non è più mappata
all'avvio, il doppio `raise` è sparito, la dipendenza da ImageMagick non c'è più, il caso "in
riproduzione" ha finalmente una prova positiva — ma hanno lasciato aperti i bordi dello stato nuovo.
`VIDEO_OPEN` è un booleano globale senza proprietario e con una sola transizione di ritorno: una
seconda apertura respinta spegne la visibilità di quella che sta riuscendo, e da lì il video resta
invisibile per il resto della sessione, cioè esattamente ciò che N2 esiste per rendere impossibile.
Le due uscite anticipate di `video_open` scrivono lo stato e non lo compensano, e nel caso del timeout
lo scrivono dopo che il chiamante ha già dichiarato il fallimento.

Il secondo bloccante è di processo e pesa quanto il primo: entrambe le correzioni al backend sono
state chieste con il loro test dalla prima passata, e il test non c'è. Il caso più frequente in
assoluto — l'app aperta senza video — non è coperto da nessuna asserzione su nessuna piattaforma,
quindi il bloccante appena chiuso può rientrare domani con la suite verde. In una consegna che è per
metà una consegna di test, questo non può passare.

Va infine detto al proprietario, perché la consegna afferma che i rilievi della prima passata sono
stati corretti tutti: non lo sono. Restano non applicati almeno il commento sbagliato su Wayland in
`env.js` (serio), le due frasi di `shell-layout.md` che ora contraddicono il codice (serio), e otto
rilievi minori dell'harness già scritti per nome. Il conto onesto di questa consegna non è "sei
bloccanti e sei seri corretti", è "corretti nel codice, in parte, e senza i test che li tengono".
