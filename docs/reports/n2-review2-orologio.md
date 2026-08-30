# N2 — seconda passata di review: l'orologio del trasporto e le correzioni della prima passata

Repo `/home/alcahest/git/SubLore`, branch `main`, lavoro non committato (`git diff HEAD` + `git status`).
Passata 2 su N2, secondo WORKFLOW.md §4b: le correzioni scritte sotto pressione di review sono codice nuovo, e questa passata cerca cosa hanno rotto.

## Come ho verificato

Letto: `src-tauri/src/video/mod.rs`, `player.rs`, `surface/{mod,linux,windows}.rs`, `src/hooks/useVideoPlayer.ts`, `src/components/{VideoControls,VideoStage,VideoOpenBar}.tsx`, `src/App.{tsx,css}`, `e2e/specs/video-surface.spec.js`, `e2e/lib/{pixels,env,proc,x11,input,paths}.js`, `e2e/wdio.conf.js`, `.github/workflows/ci.yml`, e i report della prima passata.

Misurato: ho pilotato `mpv` 0.41.0 locale via IPC con le stesse opzioni di `SAFE_OPTIONS` (`player.rs:33-51`) sulla fixture del repo, per rispondere con numeri e non con ragionamento alla priorità 3. Script usato e buttato, in scratchpad, non committato. Due risultati, riportati sotto.

Non verificato, e lo dico prima dei rilievi (CLAUDE.md §9): **non ho eseguito la batteria E2E**. Nessuna delle affermazioni qui sotto sul verde o sul rosso dei tre test è stata osservata; sono deduzioni dal codice del test e dalla semantica di `waitFor`. Le misure su mpv sono reali ma fatte con `vo=null` fuori dall'app, non con `vo=gpu` dentro la superficie X11 e non sul runner della CI.

---

## Priorità 3 del proprietario: l'orologio è davvero fermo?

### Q1 — quali proprietà mpv, con che valori, e come è osservato `time-pos`

`SAFE_OPTIONS` (`player.rs:33-51`) imposta 18 opzioni; quelle che contano per la posizione sono `pause=yes` (il file si apre fermo), `keep-open=yes` (a fine file mpv si mette in pausa invece di chiudere) e `idle=yes`. `hr-seek` **non è impostato**, resta al default.

`time-pos` è osservato come `Format::Double`, id 1 (`player.rs:193`); `pause` come `Format::Flag`, id 2 (`player.rs:195`). Un solo thread evento (`player.rs:486-535`) fa `wait_event(0.1)` e, su `PropertyChange { name: "time-pos" }`, emette `video://position` **solo se sono passati almeno 100 ms dall'ultima emissione** (`POSITION_EVENT_INTERVAL`, `player.rs:23` e `:497`).

Misurato sulla fixture (`testsrc2` a 30 fps, 60 s):

- in riproduzione mpv emette `time-pos` **ogni ~33 ms**: 60 eventi in 2 s, intervalli osservati 18-50 ms. Uno per frame.
- **da fermo non emette**: zero `property-change` su `time-pos` nei 3 s successivi al comando di pausa, e `time-pos` riletto a 3 s di distanza è bit-identico (`2.033` e `2.033`).

Quindi sì: con le proprietà che questo progetto imposta, l'orologio di mpv è davvero fermo in pausa. Nessuna opzione fra quelle attive lo fa camminare.

### Q2 — un residuo dopo la pausa esiste? rende fragile l'uguaglianza esatta?

Il residuo esiste, ed è esattamente uno. Misurato: dopo che il comando di pausa è stato scritto, arriva **un solo** evento `time-pos`, a **+0.1 ms**, che porta il valore avanti di **un frame** (ultimo valore emesso prima: `2.0`; valore a riposo: `2.033`; delta 33 ms). Poi più niente.

Non rende fragile l'uguaglianza esatta, per due motivi indipendenti:

1. arriva **prima** che il test legga `frozen`. Il test legge `frozen` (`:175`) solo dopo un'attesa a polling da 250 ms sull'etichetta del bottone (`:167-174`), quindi il residuo è già dentro `frozen`, non dopo.
2. la stringa confrontata è **floorata al secondo intero** (`VideoControls.tsx:6-11`), quindi 33 ms sono invisibili tranne che nel ~3% dei casi in cui attraversano un confine di secondo — e quel caso è comunque coperto dal punto 1.

`hr-seek` è irrilevante qui: il test non emette nessun seek.

**La conclusione è però l'opposto di quella rassicurante.** L'uguaglianza esatta non è la parte fragile; è la parte cieca. Vedi rilievi 1 e 2.

Onestà sul metodo: misurato in locale, mpv 0.41.0, `vo=null`, fuori dall'app. L'app usa `vo=gpu` dentro una finestra X11 e la CI gira su llvmpipe. Un `vo` reale può consegnare un frame in più prima di fermarsi; l'ordine di grandezza (un frame, decine di ms) non cambia, ma non l'ho misurato in quella configurazione.

### Q3 — il frontend scrive stato in modo ottimistico? `.controls__time` può cambiare senza mpv?

Sì a entrambe, e conta.

Ottimistico in due punti: `togglePlayback` scrive `paused` da sé appena l'invoke ritorna (`useVideoPlayer.ts:96`), e `seek` scrive `position` **prima** di chiamare il backend (`useVideoPlayer.ts:104`). Conseguenza diretta sul test: l'attesa che il bottone legga "Play" (`:167-174`) prova che il click è arrivato e che `video_pause` è tornato, **non** che mpv sia in pausa. L'unica prova lato mpv in quel test è l'orologio — che è appunto quello che i rilievi 1 e 2 dicono non essere provato.

`.controls__time` può cambiare senza mpv per tre vie (`VideoControls.tsx:73-75`):

- lo span rende **anche `duration`**, che viene da `state.duration` e viene riscritto per intero a ogni evento `video://state`. Nel test non si muove, ma il confronto è sull'intero `textContent`, non sulla sola posizione.
- `value = dragged ?? position` (`:32-33`): un `pointerdown` qualunque sulla slider sostituisce il valore mostrato.
- il `seek` ottimistico di cui sopra, innescato dal `commit()` su pointerup.

Nessuna delle tre è attraversata dai tre test come sono scritti oggi (il click va su `.controls__button`), quindi non è un difetto vivo. Ma il commento del test che chiama `.controls__time` "written from mpv's own `time-pos`, not from frontend intent" (`:65`) dice più di quanto sia vero, ed è la classe di commento che il proprietario ha vietato. Rilievo 9.

C'è invece un difetto vivo, e sta a monte: **il valore mostrato non è `time-pos`, è l'ultimo campione che ha superato la strozzatura, e l'ultimo campione prima della pausa viene quasi sempre buttato.** Rilievo 8.

### Q4 — `expect(await transport()).toBe(frozen)` è la forma giusta? serve una tolleranza?

Una tolleranza **no**, e aggiungerla sarebbe esattamente indebolire un'asserzione per far passare qualcosa (CLAUDE.md §5.4). Il residuo misurato non arriva mai fra la lettura e l'asserzione, quindi non c'è niente da tollerare.

La forma giusta è la stessa uguaglianza esatta, con due cose che oggi mancano:

- **risoluzione dichiarata.** Ancorare a `document.querySelector(".controls__slider").value` invece che alla stringa floorata: la slider ha `step={0.01}` (`VideoControls.tsx:65`), quindi il valore riletto dal DOM è arrotondato alla griglia dei centesimi e dà **10 ms** di risoluzione invece di 1 s. È già nel DOM, non costa niente, ed è la seconda opzione che la prima passata aveva offerto ("o sul valore di `.controls__slider`") e che non è stata presa.
- **un intervallo garantito.** Leggere il valore, aspettare deterministicamente ≥300 ms, rileggerlo e pretendere che sia identico: così il residuo di un frame cade dentro l'attesa e non fra le due letture, e l'uguaglianza resta esatta senza diventare fragile. Poi pretendere che fra quella lettura e l'asserzione finale passi almeno l'intervallo che si vuole poter smentire.

---

## Rilievi

### [BLOCCANTE] e2e/specs/video-surface.spec.js:176-179 — la prova che l'orologio è fermo non misura niente

`waitFor` valuta la probe **immediatamente** e ritorna al primo valore truthy (`e2e/lib/proc.js:15-21`: `for(;;) { const value = await probe(); if (value) return value; ... await sleep(interval) }`). Quindi `waitFor(async () => ((await transport()) === frozen ? true : null), { timeout: 5000 })` risolve alla **prima** iterazione, pochi millisecondi dopo che `frozen` è stato letto alla riga sopra. Non aspetta 5 secondi, non aspetta niente: confronta una lettura con sé stessa. Il `timeout: 5000` non è mai raggiunto e non può fallire.

Il commento sopra dice "mpv's own clock, held still. This is the proof the video is stopped, not the button." È un'intenzione, non una misura: è precisamente ciò che il proprietario vieta.

La prima passata aveva chiesto testualmente "verificare che sia ferma **per un intervallo misurabile** prima di nascondere" (`n2-review-test.md`, correzione al primo bloccante). Quella metà della correzione non è stata fatta, e la forma scelta la fa sembrare fatta.

Scenario in cui morde: mpv o il backend smettono di fermare la riproduzione alla pausa (una regressione su `set_pause`, `player.rs:431-436`, o un `keep-open` che riparte). Il caso in pausa resta verde: l'attesa passa comunque, e l'unica altra rete è l'asserzione finale di riga 196, che il rilievo 2 dimostra essere cieca.

Correzione: sostituire la `waitFor` con una lettura, un'attesa deterministica più lunga della risoluzione dell'orologio, e una `expect` di uguaglianza. Una `waitFor` con predicato di uguaglianza non può mai fallire nel verso giusto e non va usata per provare stabilità.

### [BLOCCANTE] e2e/specs/video-surface.spec.js:196 — l'asserzione finale del caso in pausa ha risoluzione di 1 secondo e nessun pavimento temporale

`transport()` legge `.controls__time`, che è `formatTime(value) + " / " + formatTime(duration)` con `formatTime` che fa `Math.floor` sui secondi (`VideoControls.tsx:6-11`, `:73-75`). La stringa cambia solo quando la posizione attraversa un secondo intero.

Quindi `expect(await transport()).toBe(frozen)` rileva un riavvio della riproduzione **solo se** fra la lettura di `frozen` (`:175`) e l'asserzione (`:196`) è trascorso abbastanza tempo da attraversare un confine di secondo. Niente nel test garantisce quel tempo: entrambe le `waitFor` intermedie (`:182` su `IsUnMapped`, `:190` sulla saturazione) possono risolvere alla prima valutazione, e le due `setStageCollapsed` sono un round trip di browser ciascuna.

Scenario concreto: il ri-show fa ripartire silenziosamente la riproduzione e la sequenza hide/show dura 0.4 s. La posizione avanza di 0.4 s, la stringa floorata cambia con probabilità 0.4, il test passa nel 60% delle esecuzioni. Un difetto che si presenta in tre esecuzioni su cinque non è sorvegliato: è una moneta.

È l'AC che il test dichiara di provare ("without restarting playback") ed è la ragione per cui il caso in pausa esiste come test separato.

Correzione: quella della Q4 sopra — `.controls__slider`.value per la risoluzione (10 ms invece di 1 s), più un intervallo minimo imposto fra la lettura e l'asserzione. Uguaglianza esatta, nessuna tolleranza.

### [BLOCCANTE] src-tauri/src/video/mod.rs:28-35 — la correzione del terzo bloccante non è coperta da nessun test, su nessuna piattaforma

Il bloccante della prima passata era: `apply_region` mostrava la superficie anche senza nessun video aperto. La correzione (`VIDEO_OPEN`) è quella giusta e il codice è corretto: a freddo `VIDEO_OPEN` è `false`, il primo aggiornamento di regione fa `set_region` e poi `set_shown(surface, false)`, quindi la superficie resta non mappata mentre `.stage__empty` è visibile.

La stessa correzione chiedeva testualmente: "Poi coprirlo: un caso E2E che all'avvio, senza aprire niente, verifica che la superficie sia `IsUnMapped` mentre `.stage__empty` è visibile." Quel caso **non esiste**. `EXPECTED_TESTS` è passato da 27 a 30 e i tre test aggiunti girano tutti a video già aperto; nessuno degli altri sei spec guarda la superficie prima di un `video_open` (verificato con grep su `mapState`/`IsUnMapped` in `e2e/specs/`).

CLAUDE.md §5.2: i test comportamentali sono lo strato primario, e una correzione a un bloccante senza copertura è una correzione che la prossima persona può disfare in silenzio.

Scenario: qualcuno inverte il senso del guard a riga 230, o lo rimuove ritenendolo ridondante. La lastra opaca sullo stage vuoto all'avvio torna, e i 30 test restano verdi.

Correzione: è scrivibile subito. All'avvio la superficie esiste già come figlia X11 del toplevel con la geometria dello stage (la regione iniziale non è vuota, quindi `set_region` gira) e semplicemente non è mappata. Basta un `it` all'inizio di `video.spec.js`, prima di aprire la fixture, che trovi il figlio > 50x50 e asserisca `mapState(...) === "IsUnMapped"` più la presenza di `.stage__empty`.

### [SERIO] src-tauri/src/video/mod.rs:125-131 — `video_open` mostra la superficie senza guardare la regione, e rompe l'invariante che la correzione dichiara

Il commento a `:29-31` dice "Visibility follows this and never the geometry". Non è vero come scritto: `apply_region` dà alla geometria un veto (`:226-228`, regione vuota → nascondi). L'invariante reale è "visibile se e solo se un video è aperto **e** la regione non è vuota" — tranne dentro `video_open`, che chiama `set_shown(surface, true)` senza consultare la regione.

Scenario: stage collassato (regione vuota, `SHOWN=false`), l'utente apre un file. `video_open` mappa la superficie alla geometria **vecchia**, sopra uno stage collassato. Nessun aggiornamento di regione segue, perché il `ResizeObserver` di `VideoStage.tsx:41-44` scatta solo sui cambiamenti, e la geometria non è cambiata. La lastra resta finché non capita un altro layout. È la stessa classe di difetto che la correzione doveva chiudere, spostata in un altro momento.

C'è una tensione vera dietro, e va scritta invece che lasciata implicita: nascondere durante il load sarebbe peggio, perché mpv costruisce il proprio output dentro la superficie e lo lascia non mappato se la superficie è nascosta (`surface/mod.rs:85-86`, e il commento a `mod.rs:123-124`). La scelta fatta è quindi la meno peggio — ma non è dichiarata, non è coperta, e il commento afferma il contrario di quello che il codice fa.

Correzione: tenere il terzo stato (`REGION_EMPTY`) e decidere la visibilità in un punto solo. Se si vuole comunque forzare lo show durante il load, dirlo nel commento e rimettere subito dopo l'esito un `set_shown` calcolato sui due flag.

### [SERIO] src-tauri/src/video/mod.rs:138-144 — la correzione rende permanente un nascondimento che prima si auto-guariva

Il percorso d'errore di `video_open` adesso spegne anche `VIDEO_OPEN`. Prima chiamava solo `surface.hide()`, e il primo aggiornamento di regione successivo rimetteva la superficie a posto. Adesso non più: con `VIDEO_OPEN=false` ogni `apply_region` successivo la lascia nascosta per sempre.

Il problema è che `video_open` tratta "l'apertura è fallita" come "nessun video è aperto", e c'è almeno un ramo dove non è vero. `Player::open` esce presto con `command_failed("another open is already in progress")` (`player.rs:246-250`) **senza toccare lo stato del player**: quel ramo significa "un'altra apertura sta lavorando", non "non c'è niente aperto".

Scenario concreto e raggiungibile: il bottone Open è disabilitato solo su `state.status === "loading"` (`App.tsx:49`), e quello stato arriva via evento dal backend (`useVideoPlayer.ts:53-55`), non viene scritto localmente prima dell'invoke (`:73-87`). Fra il click e l'arrivo dell'evento il bottone è ancora abilitato: un doppio click veloce manda due `video_open`. Il secondo prende il ramo di cui sopra, mette `VIDEO_OPEN=false` e nasconde. Il primo riesce: il player va Ready, i controlli si abilitano, l'orologio cammina, l'audio si sente — e lo stage resta nero per sempre, perché nessun resize può più rimostrare la superficie. Sintomo classificabile come "il video non parte", causa a due comandi di distanza.

Correzione: non derivare `VIDEO_OPEN` dal risultato della chiamata ma dallo stato reale del player, oppure spegnerlo solo per gli errori che hanno davvero riportato il player a idle. In alternativa, serializzare le aperture nel frontend prima di mandarle.

### [SERIO] e2e/specs/video-surface.spec.js:217 — l'ultima asserzione del terzo test misura i pixel senza attendere il ridisegno

`expect(saturation(surface)).toBeGreaterThan(PICTURE)` gira subito dopo la `waitFor` su `IsViewable` dell'ultimo ciclo. Mappare una finestra X11 non significa che mpv abbia già ridisegnato: senza backing store il contenuto si perde all'unmap e torna solo quando mpv serve l'Expose. Gli altri due casi fanno esattamente la stessa misura **dentro** una `waitFor` con deadline (`:120`, `:145`, `:190`); questo la fa nuda.

Scenario: runner lento con llvmpipe, ffmpeg attacca la grab prima che mpv abbia ridisegnato, `saturation` legge la finestra nera e il test diventa rosso senza che nulla sia rotto. Un rosso intermittente su un test comportamentale costa più della sua copertura, perché insegna a rieseguire.

Correzione: la stessa `waitFor` usata negli altri due casi.

### [SERIO] e2e/specs/video-surface.spec.js:66-68 e :196 — `transport()` restituisce `null` sull'elemento mancante, e l'asserzione più forte del file diventa `null === null`

`document.querySelector(".controls__time")?.textContent ?? null`: se lo span non c'è, `frozen` è `null` e `expect(await transport()).toBe(frozen)` è `null === null`, verde.

Scenario: un refactor del transport rinomina la classe o incapsula il tempo in due span. I tre test restano verdi, l'unica prova sull'orologio del caso in pausa sparisce senza rumore, e per giunta la `waitFor` di riga 176 (già un no-op, rilievo 1) confermerebbe la sparizione come stabilità. È la classe di difetto che il template di review cita per esperienza diretta: asserzioni che non asseriscono nulla.

`setStageCollapsed` la gestisce nel modo giusto due funzioni più su (`:47-49`, lancia con un messaggio esplicito); `transport()` no.

Correzione: far lanciare `transport()` sull'elemento mancante, oppure asserire il formato di `frozen` (`expect(frozen).toMatch(/^\d+:\d\d \/ \d+:\d\d$/)`) prima di usarlo come riferimento.

### [SERIO] src-tauri/src/video/player.rs:23 e :491-501 — l'ultimo valore di `time-pos` prima della pausa viene buttato e mai riemesso

Misurato: mpv emette `time-pos` ogni ~33 ms in riproduzione, e dopo il comando di pausa manda **un solo** evento residuo, a +0.1 ms, con il valore finale (`2.033` contro `2.0` dell'ultimo precedente).

La strozzatura a 100 ms **scarta, non rimanda**: `if last_position.elapsed() >= POSITION_EVENT_INTERVAL` salta l'emissione e l'evento è perso. Con eventi ogni 33 ms, due su tre sono già scartati in riproduzione; e quello residuo dopo la pausa cade quasi sempre dentro la finestra dei 100 ms aperta dall'ultima emissione. Da fermo non ne arrivano altri (misurato: zero in 3 s), quindi **non viene mai recuperato**.

Conseguenza: la posizione mostrata durante una pausa può restare indietro fino a ~100 ms (tre frame) rispetto alla posizione vera di mpv, per tutta la durata della pausa. Non rompe questi test, perché la stringa è floorata al secondo; ma è un difetto di prodotto visibile al primo utente che metta in pausa vicino a un confine di secondo e legga un numero diverso da quello del frame che sta guardando — e diventerà una discrepanza reale quando la posizione servirà per posare un cue.

È anche il motivo per cui il commento del test che chiama `.controls__time` "l'orologio scritto da `time-pos` di mpv" non regge: è l'ultimo campione sopravvissuto alla strozzatura.

Correzione: emettere sempre l'ultimo valore quando `pause` diventa vero (nel ramo `PropertyChange { name: "pause" }`, che c'è già a `:502-511`, basta rileggere `time-pos` ed emetterlo), invece di lasciarlo perso.

### [MINORE] e2e/specs/video-surface.spec.js:65 — il commento su `.controls__time` afferma più di quanto misuri

"The transport clock, which is written from mpv's own `time-pos`, not from frontend intent." Lo span rende anche `duration`, e il valore mostrato può venire da `dragged` o dalla scrittura ottimistica di `seek` (`VideoControls.tsx:32-33`, `useVideoPlayer.ts:102-110`). Nessuno di questi percorsi è attraversato dai tre test, quindi non è un difetto vivo — è un commento che dichiara un'invariante che il codice sotto non garantisce. Correzione: dire cosa è davvero (l'ultima posizione ricevuta da mpv, arrotondata al secondo) e cosa la può muovere.

### [MINORE] e2e/specs/video-surface.spec.js:213-216 — il test "senza perdere una superficie" asserisce una tautologia, ancorata a un dettaglio di GTK

Il commento dice "a show that created a window instead of mapping the old one would leave the extras behind". Quella cosa non può succedere: `VideoSurface::create` è chiamata solo in `setup` (`mod.rs:87`) e `show()` è `window.show()` su una `GdkWindow` che esiste già (`surface/linux.rs:70-73`). Il ciclo non ha nessun percorso che crei finestre.

In più `expect(remaining).toHaveLength(1)` lega il verde a quanti figli X11 nativi > 50x50 ha il toplevel, cioè a un dettaglio interno di GTK3 e WebKitGTK, non al prodotto: una versione futura che realizzi un altro figlio nativo lo fa diventare rosso per un motivo che non c'entra.

Correzione: asserire che l'id della finestra superficie dopo i dieci cicli sia lo stesso di prima del ciclo. È la stessa proprietà, misurata sul prodotto invece che sull'ambiente.

### [MINORE] e2e/specs/video-surface.spec.js — manca ancora l'`afterEach` che ripristina lo stage, chiesto dalla prima passata

Nessun `after` né `afterEach` nel file (verificato con grep). Nell'ordine attuale i test si auto-guariscono, perché chi trova lo stage già collassato lo ricollassa e l'attesa su `IsUnMapped` passa subito, quindi il costo pratico oggi è basso — più basso di quanto la prima passata stimasse. Ma era un rilievo aperto e non è stato né fatto né motivato, e WORKFLOW.md §2 punto 5 chiede che un rilievo non corretto sia dichiarato tale. Correzione: l'`afterEach` di una riga, oppure una riga nella descrizione della consegna che dice perché non serve.

### [MINORE] e2e/specs/video-surface.spec.js:45-53 — `setStageCollapsed` restituisce l'altezza misurata e nessuno la guarda

La funzione ritorna `element.getBoundingClientRect().height` e tutte e sei le chiamate scartano il valore. Un collasso inefficace (domani `.stage__surface` prende un `flex: 1` e `height: 0px` non basta più) fallirebbe più tardi, sull'attesa di `IsUnMapped`, accusando la superficie invece del DOM. Correzione: asserire il ritorno, 0 quando collassato e > 0 quando ripristinato. Costa una riga e sposta il fallimento sulla causa.

### [MINORE] e2e/lib/pixels.js:43-72 — l'exit status di ffmpeg non è mai controllato

`spawnSync` viene lanciato, si controlla `run.error` (processo non partito) e si cerca `SATAVG` nell'output unito, ma `run.status` non viene guardato. Se ffmpeg esce non-zero avendo comunque stampato una riga `signalstats` (grab parziale, regione che esce dallo schermo, X server che si muove sotto), il numero viene usato come misura buona. Correzione: fallire su `status !== 0` riportando stderr, prima del parse.

### [MINORE] src-tauri/src/video/mod.rs:39-50 — la sola logica di visibilità del prodotto non ha copertura unitaria né copertura Windows

`set_shown` e la decisione a `:226-230` sono ora una macchina a due flag, e sono raggiungibili solo attraverso una `VideoSurface` reale: nessun test Rust le tocca, e gli E2E che le esercitano girano solo su Ubuntu (`.github/workflows/ci.yml`, job `e2e: runs-on: ubuntu-latest`; il job `check` su Windows compila e lancia `cargo test`, non gli E2E). CLAUDE.md §5.5.

Il rischio reale è basso, perché il cambiamento è tutto nel Rust condiviso e i backend piattaforma (`show`/`hide`) non sono stati toccati. Ma la consegna non lo dice da nessuna parte, e §9 chiede che il non verificato sia presentato come non verificato.

Correzione a costo quasi nullo: estrarre `fn should_show(video_open: bool, region_empty: bool) -> bool`, chiamarla dai due punti, e coprirne i quattro casi in `cargo test` — che gira su entrambe le piattaforme della matrice.

---

## Cosa ho controllato e escluso, con il motivo

- **ImageMagick → ffmpeg (il primo bloccante della prima passata).** Davvero risolto. `ffmpeg` è nell'`apt-get install` di entrambi i job della CI (`check` e `e2e`), `requireFfmpeg` (`pixels.js:21-30`) fallisce con il proprio nome e le istruzioni di installazione, e la misura scelta (SATAVG) è quella che separa i due stati con il margine dichiarato. Nessun rilievo.
- **`EXPECTED_TESTS` 27 → 30.** Contati i blocchi `it` in `e2e/specs/`: esattamente 30. Il numero è esatto, non lascia margine. Nessun rilievo.
- **Doppio `raise` per aggiornamento di regione** (minore della prima passata). Risolto come effetto del memo `SHOWN`: `set_region` alza una volta e `set_shown` esce subito quando lo stato non cambia, quindi `show()` (che alza di nuovo) non viene più richiamato a ogni frame. Nessun rilievo.
- **Ordine al ri-show.** `apply_region` fa `set_region` **prima** di `set_shown(true)` (`:229-230`): la superficie viene spostata mentre è ancora non mappata, quindi non c'è un lampo alla geometria vecchia. Corretto anche su Windows, dove `SetWindowPos` su una finestra nascosta è legittimo e `ShowWindow` viene dopo. Nessun rilievo.
- **Stato iniziale di `SHOWN`.** `SHOWN = false` non mente: la `GdkWindow` figlia nasce non mappata (`gdk::Window::new`, `surface/linux.rs:37`, nessuna `show()` in `create`) e l'HWND è `WS_CHILD` senza `WS_VISIBLE` (`surface/windows.rs:34`). Il memo parte allineato alla realtà. Nessun rilievo.
- **`set_shown` e i fallimenti.** Il `?` esce prima di aggiornare `SHOWN` (`:43-48`), quindi un `show`/`hide` fallito non fa divergere il memo dalla realtà. Ordine corretto. Nessun rilievo.
- **Primo test, caso in riproduzione.** La prova che la riproduzione è partita è positiva e onesta: l'orologio deve cambiare, e con `pause=yes` all'apertura (`SAFE_OPTIONS`) non può cambiare da solo. La granularità di 1 secondo qui non fa danno, perché il predicato è "diverso da", non "uguale a", e la deadline di 15 s copre ampiamente il secondo necessario. Nessun rilievo.
- **Pinning di `GDK_BACKEND` e rimozione di `WAYLAND_DISPLAY`.** Chiude il buco che il probe aveva documentato, ed è applicato in tutti e tre i punti che lanciano l'app: `wdio.conf.js:20-23`, `shutdown-check.js`, `close-gate-check.js` via `lib/env.js`. Nessun rilievo.
- **`hr-seek` e le altre proprietà ai default.** Nessun seek è emesso in questo spec, quindi non c'è percorso attraverso cui possano muovere la posizione. Escluso per assenza di innesco, non per assenza di rischio.

---

## Verdetto

**RICHIEDI MODIFICHE.**

Tre bloccanti, tutti sulla stessa cosa e tutti prodotti dalla passata di correzione, non dal lavoro originale.

Il caso in pausa — quello che il probe indica come il caso che l'utente incontra davvero — continua a non avere una prova positiva che il video sia fermo. La prima passata aveva detto dove prenderla e con quali due forme; la correzione ha preso la meno adatta delle due (la stringa floorata al secondo invece del valore della slider a centesimi) e ha implementato l'attesa di stabilità con una `waitFor` che ritorna alla prima valutazione. Il risultato è un test che _sembra_ misurare due cose e non ne misura nessuna: la stabilità è un confronto di un valore con sé stesso, e l'asserzione finale è una moneta la cui probabilità di uscire dalla parte giusta dipende da quanto ha impiegato ffmpeg. Il terzo bloccante è la copertura mancante sulla correzione al bloccante precedente, chiesta esplicitamente e non fatta.

Sul resto: la macchina a due flag in `mod.rs` è la struttura giusta per il difetto giusto, e il memo `SHOWN` risolve di sorpresa anche il doppio `raise`. Ma la macchina ha una transizione incoerente (`video_open` che ignora la regione) e un percorso d'errore che ha trasformato un nascondimento transitorio in uno permanente. Nessuno dei due è coperto, e la logica è raggiungibile solo da un E2E che gira su una sola delle due piattaforme della matrice.

La misura su mpv risponde alla domanda del proprietario in modo netto e va tenuta: con le proprietà che questo progetto imposta, `time-pos` da fermo non si muove — un solo evento residuo, un frame, a 0.1 ms dalla pausa. L'uguaglianza esatta non è fragile. Il problema è che non è nemmeno una misura.
