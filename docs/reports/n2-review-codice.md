# N2 — review del codice (lente: il codice e i suoi effetti)

Revisione del diff non committato su `main`, 2026-08-30. Superficie esaminata:
`src-tauri/src/video/mod.rs`, `e2e/specs/video-surface.spec.js` (nuovo), `e2e/lib/env.js` (nuovo),
`e2e/wdio.conf.js`, `e2e/scripts/shutdown-check.js`, `e2e/scripts/close-gate-check.js`,
`docs/design/shell-layout.md`, `docs/reports/n2-probe.md`.

Contorno letto per capire gli effetti: `video/surface/{mod,linux,windows}.rs`, `main.rs`, `lib.rs`,
`video/player.rs`, `src/components/VideoStage.tsx`, `src/hooks/useVideoPlayer.ts`, `src/App.tsx`,
`src/App.css`, `e2e/lib/{x11,proc,paths,input,driver}.js`, `e2e/specs/video.spec.js`,
`e2e/README.md`, `.github/workflows/ci.yml`, `BACKLOG.md` N2, `CLAUDE.md`, `WORKFLOW.md` §4b.

## Come sono state ottenute le prove

Il binario `target/debug/sublore` è stato compilato alle 00:14:42, dopo la modifica a
`video/mod.rs` (00:13:16), quindi contiene il cambiamento. Ho lanciato quel binario sotto Xvfb
1024x700 con `GDK_BACKEND=x11` e `WAYLAND_DISPLAY` rimosso, **senza aprire alcun video**, e ho
letto l'albero X11 e i pixel. Poi ho ripetuto con il binario pre-modifica conservato in
scratchpad. Le misure sono riportate per intero sotto il rilievo 1. Dove non ho potuto misurare lo
dico esplicitamente (CLAUDE.md §9).

---

## Rilievi

### [BLOCCANTE] src-tauri/src/video/mod.rs:203 — la superficie viene mostrata all'avvio, senza alcun video aperto, e copre il segnaposto

`apply_region` chiama `show()` su qualunque regione non vuota. Ma `VideoStage` è montato sempre
(`App.tsx:73`, senza condizione su `ready`), il suo effetto chiama `schedule()` al mount
(`VideoStage.tsx:44`) e `.stage__surface` ha una geometria reale anche a video chiuso
(`.stage { flex: 1 1 45%; min-height: 120px }`, `.stage__surface { width:100%; height:100% }`,
`App.css:100-111`). Quindi la primissima `video_set_region` dopo il primo paint arriva con una
regione non vuota, e da oggi mappa la superficie.

Scenario concreto: l'utente lancia Sublore e non apre nulla. Misurato, binario con la modifica,
nessun video aperto:

```
0x200004 "Sublore": 1024x700+0+0
   0x200023 (has no name): 736x159+288+296    Map State: IsViewable
```

e il ritaglio di quel rettangolo dallo screenshot della root è una lastra piatta:
`stddev=0`, colore unico `#202326`. Lo screenshot completo conferma: il pannello video è un
rettangolo grigio uniforme, la scritta **"No video open." non è visibile** e nemmeno lo sfondo
`#000` di `.stage`.

Stesso binario pre-modifica, stesso ambiente, nessun video aperto:

```
   0x200023 (has no name): 1x1+0+0            Map State: IsUnMapped
```

Il meccanismo è univoco anche solo leggendo il codice: `VideoSurface::create` non mostra mai
(`surface/mod.rs:68`), `Surface::set_region` fa `move_resize` + `raise` e nessuna delle due mappa
una finestra X11, e prima di questo diff `show()` aveva un solo chiamante, `video_open`. Lo dicono
anche i due documenti spediti in questo stesso diff: `shell-layout.md` ("set_region never maps a
hidden window") e `n2-probe.md`.

Peggiora in tre modi:

1. È una regressione visibile nel prodotto libero, al primo frame, per ogni utente che apre l'app
   senza video.
2. Contraddice il contratto scritto dal repo stesso, `e2e/README.md:44-47`: "The surface exists and
   is already sized **before** any video is opened; it is only _mapped_ once a video is ready, so
   the `IsViewable` check is what makes this test meaningful." Il README non è stato aggiornato.
3. Indebolisce un'asserzione esistente senza toccarla: `video.spec.js:126,136` verifica
   `mapState === "IsViewable"` sulla superficie dopo l'apertura del video. Ora quella condizione è
   vera fin dall'avvio, quindi il controllo non può più fallire per il motivo per cui era stato
   scritto. È esattamente il caso che CLAUDE.md §5.4 vieta, e la stessa classe di difetto che
   `shell-layout.md` in questo diff descrive per `.stage__empty`.

Correzione: separare _dove_ da _se_. La visibilità diventa uno stato del modulo video (impostato a
true da `video_open` riuscito, a false dal fallimento di `video_open` e, in futuro, dal comando che
la decisione 1 introdurrà), e `apply_region` chiama `show()` solo quando quello stato è true.
Questo mantiene la decisione 2 del proprietario (la superficie segue la regione in entrambe le
direzioni per un video caricato) e chiude anche i rilievi 4 e 5. La stessa regressione esiste su
Windows per costruzione (`windows.rs:75-81`, `ShowWindow(SW_SHOWNA)` su una `STATIC` figlia sopra
WebView2) e lì non è né testata né misurata: nessun job E2E gira su Windows in `ci.yml`.

### [BLOCCANTE] e2e/specs/video-surface.spec.js:33,36 — dipendenza non dichiarata da ImageMagick; il test non può girare in CI

`spread()` esegue `import -window root` e `magick`. Nessuna delle due è installata dal job
`e2e smoke (ubuntu)`: la lista `apt-get install` (`ci.yml:135-152`) contiene `x11-utils`,
`xdotool`, `xvfb`, e nient'altro di ImageMagick. Peggio, su `ubuntu-latest` il pacchetto
`imagemagick` è ImageMagick 6, che fornisce `import` e `convert` ma **non** il binario `magick`
(introdotto in IM7). Qui in locale funziona solo perché Fedora ha ImageMagick 7.1.2 installato.

Scenario concreto: si fa push, il job Linux esegue `pnpm e2e`, il `before` della nuova spec muore
in `spawnSync magick ENOENT` e i tre test falliscono. CLAUDE.md §5.5 chiede la matrice verde prima
di un tag; questo diff la rende rossa.

In più viola la convenzione del harness: `paths.js:25` stabilisce che "Missing prerequisites are
failures with an actionable message, never skips", e `wdio.conf.js:44-51` chiama `requireDisplay`,
`requireAppBinary`, `requireVideoFixture` in `onPrepare` proprio per questo. Le due nuove
dipendenze non hanno alcun `require*`, quindi il fallimento arriva come ENOENT invece che come
"installa ImageMagick". `e2e/README.md:64-68` elenca i prerequisiti e non è stato aggiornato.

Correzione: aggiungere un `requireImageMagick()` in `paths.js` chiamato da `onPrepare`, aggiungere
il pacchetto alla lista apt di `ci.yml` con un binario che esista su Ubuntu (`import` + `convert`,
oppure `imagemagick` e uso di `convert ... -format '%[fx:standard_deviation]' info:` invece di
`magick`), e aggiornare l'elenco prerequisiti nel README.

### [BLOCCANTE] e2e/specs/video-surface.spec.js:114-164 — nessuna prova positiva che il video stesse riproducendo o fosse in pausa, e nessuna prova che la riproduzione continui

Regola del proprietario del 2026-08-29: un test comportamentale deve produrre prova positiva che
l'azione è avvenuta. I due primi test prendono il nome dalla loro precondizione e non la
verificano mai.

Test 1, "with the video playing" (righe 114-120). Clicca `.controls__button` e poi attende
`spread(surface, "play-before") > PICTURE`. Ma `before()` (righe 108-111) ha già atteso
`spread > PICTURE` sulla stessa superficie: l'attesa è soddisfatta al primo poll qualunque cosa
abbia fatto il clic. Scenario concreto: il clic manca il bottone (coordinate calcolate su un
`getBoundingClientRect` letto prima, nessuna attesa che il bottone sia abilitato). Il test passa,
verde, e ha esercitato hide/show su un video **in pausa** — cioè è diventato un duplicato del
test 2, mentre il rapporto dirà che il caso "in riproduzione" è coperto.

Test 2, "with the video paused, without restarting playback" (righe 137-164). L'etichetta è letta
_dopo_ il clic (righe 140-143) e alla fine si asserisce solo che non è cambiata (riga 163). Se il
clic non è arrivato, l'etichetta non cambia mai, l'asserzione passa, e il video era in
riproduzione per tutto il tempo. L'asserzione non distingue "in pausa" da "in riproduzione": è
vera in entrambi i casi. Inoltre `waitFor` a riga 140 ritorna al primo poll, perché `textContent`
è sempre una stringa non vuota; il messaggio "the transport label to settle" descrive un'attesa
che non avviene mai, ed espone anche una corsa nella direzione opposta (se l'etichetta si
aggiorna _dopo_ la lettura, il test fallisce senza motivo reale).

Terzo buco, che tocca direttamente l'AC scritta. `BACKLOG.md:83` chiede: "open a video, hide the
surface, show it again: the frame is visible **and playback continues**". Nessuno dei tre test
asserisce che la riproduzione continui. `spread > 0.05` distingue "qualcosa disegnato" da "buco
vuoto", non "fotogramma vivo" da "fotogramma congelato". La sonda aveva la misura giusta e il
test non l'ha ereditata: `n2-probe.md` campiona due volte a 1,5 s di distanza e mostra
0.3832 → 0.3850, cioè che i fotogrammi avanzano. Anche la seconda AC (`BACKLOG.md:84`) è coperta a
metà: "no leaked surface" è asserito (riga 183), "no orphan process" non è controllato da nulla.

Scenario concreto complessivo: mpv rimappa la finestra ma non riprende a decodificare. Tutti e tre
i test passano; l'utente riapre il menu e vede un fermo immagine.

Correzione: (a) provare la precondizione, non assumerla — dopo il clic attendere che
`.controls__button` mostri il testo atteso (`en.video.pause` per "in riproduzione",
`en.video.play` per "in pausa") prima di misurare; (b) per il caso in riproduzione, dopo il
re-show campionare `spread` due volte a distanza e pretendere che il valore cambi, e in più che
`.controls__time` sia avanzato — questo copre "playback continues"; (c) per il caso in pausa,
pretendere il contrario: `.controls__time` identico prima e dopo, che è la prova positiva che
nulla è stato riavviato, molto più forte dell'etichetta; (d) aggiungere al test dei dieci cicli il
controllo processi che l'AC chiede (`processGroupMembers` esiste già in `lib/proc.js`).

### [SERIO] src-tauri/src/video/mod.rs:202-203 — la regione ora rialza _e mostra_: la decisione 1 nasce già rotta, e il documento di design spedito nello stesso diff dice il contrario

Prima del diff, una superficie nascosta restava nascosta sotto un aggiornamento di regione:
`set_region` fa `move_resize` + `raise` (`linux.rs:65-66`), e `raise` su una finestra non mappata
non la mappa. Il rischio della decisione 1 era limitato a un restacking invisibile. Da oggi ogni
aggiornamento di regione **mappa** la superficie.

Scenario concreto, ed è esattamente quello che la decisione 1 dovrà evitare: si apre un menu HTML,
la shell nasconde la superficie, l'utente ridimensiona la finestra (o qualunque cosa muova il
layout: una banda di errore che appare, il pannello progetto che cambia altezza). Parte una
`video_set_region` con regione non vuota, e il video ricompare **sopra il menu aperto**, che è
figlio del webview e quindi sotto la superficie X11 per costruzione.

Il difetto è aggravato dal fatto che `docs/design/shell-layout.md`, modificato in questo stesso
diff, progetta M2.0 su una premessa che questo commit ha appena invalidato:

- "any region update while a layer is open **raises** the surface again (`surface/linux.rs:66`)" —
  ora la mostra, non solo la rialza.
- "`set_region` never maps a hidden window, so sending a rectangle as the way back would move an
  unmapped window and show nothing" — è falso per il percorso `apply_region`, che è l'unico che il
  frontend può raggiungere.

Il documento mitiga già la cosa dal lato frontend ("While the layer set is non-empty the shell
holds the last measured rectangle and sends nothing"), ma questo introduce nel backend un
invariante che nulla impone: "non mandare mai una regione mentre un livello è aperto". Un solo
punto del frontend che dimentichi la regola riporta il video sopra il menu, e il backend non ha
modo di accorgersene.

Correzione: la stessa del rilievo 1 (stato di visibilità esplicito nel backend). Con quello,
mandare una regione mentre un livello è aperto diventa innocuo, l'invariante non serve più, e le
due frasi di `shell-layout.md` vanno riscritte perché oggi descrivono codice che non esiste più.

### [SERIO] src-tauri/src/video/mod.rs:116 vs 203 — dopo un'apertura fallita, il primo resize riporta la superficie vuota sullo schermo

`video_open` nasconde la superficie quando l'apertura fallisce (riga 116), ed è giusto: il
messaggio d'errore deve essere leggibile. Ma quel `hide` non è più stabile.

Scenario concreto: l'utente sbaglia percorso o il file è in un formato che libmpv rifiuta. Compare
la banda `.app__error` (`App.tsx:50-54`). La comparsa della banda cambia il layout della colonna,
`ResizeObserver` su `.stage__surface` scatta, parte una regione non vuota, e la lastra vuota
ricompare immediatamente sopra il segnaposto — con l'aggravante che qui il rimappaggio è causato
proprio dall'errore che stiamo mostrando. Stessa cosa a ogni successivo ridimensionamento della
finestra.

Correzione: come sopra. `video_open` in errore imposta lo stato di visibilità a false, e
`apply_region` lo rispetta.

### [SERIO] e2e/lib/env.js:6-13 — la diagnosi scritta nel commento è sbagliata, e nasconde un difetto vivo del prodotto sulla sessione reale del proprietario

Il commento dice: "GTK prefers Wayland whenever `WAYLAND_DISPLAY` is set, even with `DISPLAY`
pointing at an Xvfb server. mpv then never attaches to the X11 surface it was handed".

GTK non può aver scelto Wayland in quelle esecuzioni: `src-tauri/src/main.rs:6-9` imposta
`GDK_BACKEND=x11` come prima istruzione di `main`, prima di `sublore_lib::run()` e quindi prima di
`gtk_init`, con un commento che dice perché. Se GTK fosse stato su Wayland, `create()` in
`linux.rs:39-49` avrebbe fallito (`ensure_native` o il downcast a `X11Window`) e l'app avrebbe
riportato `player_unavailable`; invece la sonda descrive una superficie X11 esistente,
`IsViewable`, con zero figli. Il componente che ha ignorato il `wid` non è GTK: è libmpv.
`player.rs:186-187` imposta solo `wid` e non fissa né `vo` né `gpu-context`, quindi mpv sonda i
contesti in automatico e con `WAYLAND_DISPLAY` presente può agganciare il compositore Wayland,
dove `--wid` non ha alcun significato e viene ignorato.

Perché conta, e non è pedanteria: **la sessione del proprietario è Wayland.** Verificato in questo
ambiente: `WAYLAND_DISPLAY=wayland-0`, `XDG_SESSION_TYPE=wayland`. L'app forza GTK su XWayland e
la superficie X11 nasce correttamente, ma `WAYLAND_DISPLAY` resta nell'ambiente del processo e
libmpv vede esattamente le stesse condizioni che hanno bruciato due esecuzioni della sonda. Cioè:
è plausibile che sul desktop reale del proprietario il video non si veda affatto, e questo diff
sposta la difesa nel harness (`delete WAYLAND_DISPLAY`) invece che nel prodotto, rendendo il caso
per costruzione irriproducibile dalla suite.

Non l'ho misurato: verificarlo richiede lanciare l'app sul compositore vivo del proprietario, cosa
che apre una finestra sul suo schermo, e non l'ho fatto di mia iniziativa. È un ragionamento sul
codice più i dati già registrati in `n2-probe.md`, non una misura. Va verificato con una singola
esecuzione:
`env -u GDK_BACKEND ./target/debug/sublore` in una sessione Wayland, aprendo `sample.mkv`.

Nota secondaria che discende dalla stessa analisi: la riga `GDK_BACKEND: "x11"` in `env.js:16` e in
`wdio.conf.js:22` è ridondante — il binario se lo imposta già da solo — quindi l'unica parte
portante è la cancellazione di `WAYLAND_DISPLAY`. Configurazione inerte più una spiegazione
sbagliata è la combinazione che fa cancellare la riga giusta al prossimo che passa di qui.

Correzione: se la verifica conferma, la riga va accanto a `main.rs:9` (rimuovere `WAYLAND_DISPLAY`
dall'ambiente del processo, oppure fissare `gpu-context` in `PlayerConfig::embedded`), il commento
di `env.js` va riscritto per attribuire il comportamento a libmpv, e il harness deve poter
_non_ cancellare la variabile dietro un interruttore, così che il caso Wayland resti verificabile.

### [MINORE] src-tauri/src/video/surface/mod.rs:85-86 — il commento su `show()` descrive un vincolo che ora vale solo per un chiamante su due

"Must happen before mpv builds its video output: mpv creates its own window inside this one and
leaves it unmapped if this one is."

Risposta alla domanda posta: **il vincolo non è violato.** Riguarda la costruzione, non il ciclo di
vita: mpv crea la propria finestra figlia durante il caricamento del file, e se in quel momento il
genitore è non mappato la figlia nasce non mappata. Rimappare più tardi non ricrea nulla, e la
sonda lo conferma su entrambi i percorsi (riproduzione e pausa, `n2-probe.md` casi A e B).

Resta però un commento che oggi si legge come precondizione di _ogni_ chiamata, mentre è una
regola di ordinamento interna a `video_open`. Il rischio concreto: il prossimo che legge
`apply_region` conclude che la nuova `show()` viola il contratto e la toglie, o al contrario la
copia altrove convinto che basti. Il commento va riscritto dicendo che il vincolo è
sull'ordinamento in apertura.

Nella stessa voce: `n2-probe.md:38` chiedeva esplicitamente una correzione ai commenti di
`surface/mod.rs:82-84` e `video.spec.js:114` (la finestra figlia di mpv esiste solo dopo
l'aggancio, quindi è il segnale onesto; lo stato di mappa da solo non lo è). La correzione non è
stata applicata in questo diff, pur essendo l'unica cosa che la sonda aveva chiesto al codice.

### [MINORE] src-tauri/src/video/mod.rs:202-203 e surface/linux.rs:65-73 — doppio raise per aggiornamento: costo reale trascurabile, ma è il sintomo del rilievo 1

Risposta alla domanda posta, con il ragionamento invece della misura, perché misurare il costo
richiederebbe strumentare l'IPC e nessun budget §7 copre il resize.

Frequenza: `VideoStage.tsx:34-39` accorpa `ResizeObserver`, `window.resize` e il mount in un solo
`requestAnimationFrame`, quindi al massimo **una** `video_set_region` per frame, cioè ~60/s durante
un trascinamento continuo. Non è un flusso non limitato.

Costo per chiamata, dopo il diff: `move_resize` (XConfigureWindow) + `raise` (XRaiseWindow) +
`show` → `gdk_window_show` (XMapWindow, no-op sul server se già mappata) + un secondo
`raise` (XRaiseWindow). Sono quattro richieste X11 asincrone invece di due: nessun round trip,
nessuna risposta attesa. XRaiseWindow su una finestra già in cima non cambia l'ordine di
impilamento, quindi non genera ConfigureNotify né Expose: **niente sfarfallio e niente ridisegno**.
~240 richieste/s su un socket X11 locale non è traffico significativo. Su Windows il conto è
`SetWindowPos(HWND_TOP, SWP_NOACTIVATE)` + `ShowWindow(SW_SHOWNA)`, e `ShowWindow` su una finestra
già visibile ritorna senza fare nulla: stessa conclusione.

Quello che resta non è il costo ma la struttura: `apply_region` ha ora due responsabilità, e il
raise duplicato è il segnale che la seconda è finita nel posto sbagliato. Sparisce insieme al
rilievo 1 se la visibilità diventa stato esplicito. Non l'ho misurato con un profiler: se il
proprietario vuole il numero, il modo è contare le invocazioni `video_set_region` durante un
resize di 3 secondi e cronometrare il round trip su `on_main_thread`.

Un dettaglio adiacente che invece va sistemato comunque: ogni `video_set_region` occupa un thread
del pool `spawn_blocking` per tutta la durata del round trip (`mod.rs:163`), con timeout di 2 s.
A 60 chiamate al secondo il pool assorbe, ma un main thread momentaneamente occupato le accumula
tutte. Preesiste al diff, quindi non è un rilievo su questo cambiamento; va filato in BACKLOG.

### [MINORE] src-tauri/src/video/mod.rs:199-201 — commento di tre righe

CLAUDE.md §6 e la policy sui commenti: massimo 1-2 righe per guardia o blocco, il resto va nella
descrizione della consegna. Qui sono tre righe che rinarrano il bug ("a region that went empty once
stayed hidden for the rest of the session"), che è precisamente ciò che la policy manda nella
descrizione. Due righe bastano: cosa fa e il riferimento a N2.

### [MINORE] e2e/lib/env.js vs e2e/wdio.conf.js:20-23 — la stessa regola scritta due volte

`appEnv()` costruisce un oggetto ambiente per chi fa `spawn`; `wdio.conf.js` non può usarlo perché
tauri-driver eredita `process.env`, quindi duplica le due righe con un commento che dice "same
reason as e2e/lib/env.js". Se domani si aggiunge una terza variabile ne beneficiano solo i due
script, non la suite WebDriver. Correzione: esportare da `env.js` anche un `applyAppEnv()` che
muta `process.env`, e chiamarlo da `wdio.conf.js`. Una sola definizione, due modi di applicarla.

### [MINORE] e2e/specs/video-surface.spec.js:43-53, 92-97 — `centreOf` duplicato e apertura del fixture senza le attese che `video.spec.js` ha

`centreOf` è copiato carattere per carattere da `video.spec.js:13-23`. CLAUDE.md §6 chiede di
riusare ciò che esiste: va in `e2e/lib/`.

Più concreto: il blocco di apertura (righe 92-97) clicca il campo, digita e clicca Open senza
nessuna delle due sincronizzazioni che `video.spec.js:53-66` ha imparato a mettere — l'attesa che
`document.activeElement.className === "bar__input"` e l'attesa che il percorso digitato sia
davvero arrivato nel campo. Scenario concreto: il clic sul campo arriva un frame prima che il
webview sia pronto a prendere il focus, `xdotool type` scrive nel vuoto, Open apre stringa vuota, e
il `before` muore 30 secondi dopo con il messaggio "the surface with mpv attached inside it" —
che accusa il codice sotto esame per un difetto del harness. In un test la cui unica ragione di
esistere è distinguere "mpv non si è agganciato" da "mpv non ridisegna", è la diagnosi peggiore
possibile.

### [MINORE] e2e/specs/video-surface.spec.js:28 — la directory temporanea degli screenshot non viene mai rimossa

`mkdtempSync` a livello di modulo, nessun `after`. Ogni esecuzione lascia una directory in
`/tmp` con qualche PNG a schermo intero. Nessun danno ai dati (§3 non è toccato in nessun punto di
questo diff), ma su una macchina che gira la suite decine di volte al giorno si accumula.
Correzione: `after()` con `rmSync(SHOTS, { recursive: true, force: true })`, oppure conservarla solo
quando un test fallisce, che è anche più utile.

### [MINORE] e2e/README.md — la tabella "What each spec proves" non contiene i tre test nuovi

Il file è la mappa fra spec e criteri di accettazione, e `wdio.conf.js:12-14` rimanda proprio lì
("Bump it when you add a test; see e2e/README.md"). `EXPECTED_TESTS` è stato portato correttamente
da 27 a 30, la tabella no. Inoltre le righe 44-47 del README descrivono un comportamento della
superficie che il rilievo 1 ha cambiato.

---

## Punti guardati e chiusi senza rilievo

- **CLAUDE.md §3, sicurezza dati.** Nessun punto del diff scrive, sposta o rinomina file dell'utente.
  `apply_region` tocca solo geometria e visibilità di una finestra; la spec scrive PNG in una
  directory temporanea propria; `env.js` costruisce una copia dell'ambiente e non muta
  `process.env`. `wdio.conf.js` muta `process.env` del proprio processo, che è ciò che faceva già
  per `XDG_DATA_HOME`. Nessuna scrittura atomica, nessun backup, nessun database coinvolti.
- **`unwrap()` fuori dai test.** Nessuno introdotto: verificato sulle righe aggiunte del diff.
- **Gestione errori ai confini.** `apply_region` propaga con `?` e l'errore risale a `on_main_thread`
  e quindi all'UI via `setErrorCode` (`useVideoPlayer.ts:114-116`). Su Linux `set_region` e `show`
  sono infallibili; su Windows `set_region` può fallire e in quel caso `?` esce prima di `show`,
  senza lasciare uno stato a metà peggiore di quello di partenza.
- **Conteggio dei test.** 27 → 30 corrisponde esattamente ai tre `it(` della nuova spec, e la
  guardia `onComplete` resta un `<`, quindi continua a fallire su una spec saltata.
- **Il test esercita davvero il ramo modificato.** Collassare `height` di `.stage__surface` produce
  `getBoundingClientRect().height === 0`, che `SurfaceRegion::is_empty` legge come vuota
  (`surface/mod.rs:56-60`), e il ripristino riporta una regione reale nel ramo `else` cambiato.
  L'elemento resta renderizzato, quindi `ResizeObserver` scatta in entrambe le direzioni. Il
  percorso è quello giusto, indipendentemente dai rilievi sulle asserzioni.
- **Il terzo test (dieci cicli) è il solo dei tre con prova positiva propria.** Asserisce la
  transizione di stato in entrambe le direzioni a ogni ciclo, un solo figlio > 50x50 alla fine — e
  quel filtro è significativo, perché il toplevel ha esattamente due figli, la superficie e una
  finestra 1x1 (misurato) — e un'ultima misura di pixel. Gli manca solo il controllo processi che
  l'AC chiede, già annotato sopra.
- **`shutdown-check.js:54` e `close-gate-check.js:147`.** La sostituzione di
  `{ ...process.env, XDG_DATA_HOME: dataHome }` con `appEnv({ XDG_DATA_HOME: dataHome })` è
  equivalente più le due variabili: `appEnv` mette `overrides` dopo i default, quindi
  `XDG_DATA_HOME` continua a vincere. Nessun cambiamento di comportamento oltre a quello voluto.

---

## Verdetto

**RICHIEDI MODIFICHE.**

Tre bloccanti. Il primo è una regressione visibile nel prodotto, misurata: con la modifica in
piedi, Sublore all'avvio senza video mostra una lastra grigia al posto di "No video open.", e nel
farlo indebolisce silenziosamente un'asserzione E2E esistente e contraddice il contratto scritto in
`e2e/README.md`. Il secondo rende il job E2E Linux non eseguibile in CI per una dipendenza non
dichiarata e non installata. Il terzo è che il test nuovo non prova le proprie precondizioni e non
copre "playback continues", che è testualmente metà dell'AC di N2: verde non significa che N2 sia
verificato.

I due rilievi seri sulla decisione 1 e sull'apertura fallita hanno la stessa radice del primo
bloccante e si chiudono con la stessa correzione: la visibilità della superficie deve essere uno
stato esplicito del backend, non un effetto collaterale della geometria. Fatto quello, la decisione
2 del proprietario resta soddisfatta e la decisione 1 non nasce già rotta.

Il rilievo serio su Wayland non blocca questo diff, ma va portato al proprietario prima di
chiuderlo: il codice suggerisce che il video non funzioni sulla sua sessione reale, e questo
cambiamento al harness rende il caso irriproducibile dalla suite. Serve una singola esecuzione per
sapere se è vero.
