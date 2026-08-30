# Review N2, seconda passata — la soglia di saturazione e cosa hanno rotto le correzioni

Lente imposta da WORKFLOW.md §4b: le correzioni della prima passata (6 bloccanti, 6 seri, 23 minori) sono codice nuovo scritto sotto pressione di review, quindi qui si cerca esplicitamente cosa hanno rotto, non se hanno chiuso il rilievo che le ha generate.

Diff esaminata: `git diff HEAD` più i file non tracciati di `git status`. In scope: `src-tauri/src/video/mod.rs`, `e2e/lib/pixels.js` (nuovo), `e2e/lib/env.js` (nuovo), `e2e/specs/video-surface.spec.js` (nuovo), `e2e/wdio.conf.js`, `e2e/README.md`, i due script di `e2e/scripts/`, `docs/design/shell-layout.md`.

## Cosa ho eseguito davvero

Non ho lanciato la suite E2E. Ho lanciato **l'app vera** sotto Xvfb 1280x1024x24 con `GDK_BACKEND=x11` e senza `WAYLAND_DISPLAY`, tre volte, e ho misurato con lo stesso identico comando ffmpeg di `pixels.js`. Ho inoltre verificato ffmpeg dentro un container `ubuntu:24.04` (stessa release di `ubuntu-latest`).

Misure ottenute su questa macchina (Fedora, Mesa software, `target/debug/sublore` del 2026-08-30):

| stato                                         | rettangolo      | SATAVG       |
| --------------------------------------------- | --------------- | ------------ |
| avvio, nessun video aperto                    | 736x159+288+296 | **0.00490**  |
| video aperto, in pausa al frame 0             | stesso          | **42.6162**  |
| superficie smappata **con il video caricato** | stesso          | **0.641509** |
| rimappata subito dopo                         | stesso          | **42.6162**  |
| banda chrome in alto (1024x120+0+0)           | —               | **1.10429**  |
| banda lista battute (1024x180+0+510)          | —               | **2.00374**  |
| finestra intera, nessun video                 | 1024x700        | **1.46103**  |

Struttura X misurata all'avvio: il toplevel `0x200004` ha **due** figli, la superficie `0x200023` (736x159+288+296, `Map State: IsUnMapped`) e un 1x1 fuori schermo. Dopo l'apertura del fixture la superficie contiene `0xa00002 ("mpvk" "mpv") 736x159+0+0`, cioè mpv si aggancia davvero. Un grab ffmpeg costa **0.48 s** su questa macchina, misurato tre volte.

## Risposte alle quattro domande del proprietario

**1. ffmpeg e signalstats su `ubuntu-latest` (24.04).** VERIFICATO in container `ubuntu:24.04`: `ffmpeg 6.1.1-3ubuntu5`, `x11grab` presente (`D x11grab X11 screen capture, using XCB`), `signalstats` presente, e la chiave stampata è esattamente `lavfi.signalstats.SATAVG`. Sullo stesso input bgr0 (`testsrc2` 640x360) Ubuntu 6.1.1 e Fedora 8.1.2 danno **lo stesso identico valore, 112.603**. Il filtro e la chiave non sono un rischio. Il pacchetto `ffmpeg` è già installato da entrambi i job in `.github/workflows/ci.yml` (righe 34 e 138), quindi nemmeno la disponibilità lo è. Verificato anche il presupposto scritto in `e2e/README.md:9`: su noble `imagemagick` è `8:6.9.12.98`, cioè ImageMagick 6, che non fornisce `magick`. La motivazione della scelta di ffmpeg regge.

**2. Spazio colore e range.** `signalstats` accetta solo formati YUV, quindi ffmpeg inserisce da sé una conversione da `bgr0`; il valore dipende da matrice e range di quella conversione. Misurato, stesso frame: bt601 limited 112.603, bt601 full 128.299, bt709 limited 115.233, bt709 full 131.530, yuv420p 112.193. La banda completa è **+17%**, non un ordine di grandezza. La colorimetria non può avvicinare un margine 8x. Domanda chiusa: non è da lì che arriva il rischio.

**3. Rendering software e fixture.** Il pattern è saturo per costruzione, non per merito del decoder: `fixtures/video/make-sample.sh:5` genera `testsrc2` da lavfi e lo codifica in yuv420p, quindi i colori nascono dal generatore e sopravvivono a qualunque decodifica corretta. Il full frame legge 112.6 su entrambe le versioni di ffmpeg. Su questa macchina, con Mesa software sotto Xvfb, mpv si aggancia e disegna (misurato sopra). **Non verificato** che lo faccia sul runner Ubuntu: lì lo stack è un altro Mesa e un altro build di libmpv, e non ho modo di eseguirlo. Va detto però che quel fallimento è rosso, non verde falso: il `before` scade su "a picture on the surface before the tests begin".

**4. Il caso peggiore, e quanto è plausibile.** Perché il test dia verde falso serve che il rettangolo misurato superi 5 senza che il video sia visibile. Le sorgenti non-video misurate valgono 0.005 (stage mai usato), 0.64 (stage con video caricato ma superficie smappata) e 1.1–2.0 (chrome dell'app). Servirebbe quindi circa **8 volte il più forte segnale non-video oggi presente**, oppure una copertura del video sotto il 4.4% del rettangolo (5/112.6) perché il caso vero scenda sotto soglia — con il video che oggi ne copre il 38%, servirebbe un riquadro largo ~46 px su 736. Nessuno dei due è plausibile oggi. Ma il margine reale **non è quello scritto nel commento**: vedi il rilievo [SERIO] su `PICTURE`.

---

## Bloccanti

### [BLOCCANTE] e2e/specs/video-surface.spec.js:175-179 — l'attesa "the clock to stay put while paused" ritorna al primo poll e non asserisce niente: è lo stesso difetto che la prima passata aveva già bloccato, riscritto dentro la sua stessa correzione

```js
const frozen = await transport();
await waitFor(async () => ((await transport()) === frozen ? true : null), {
  timeout: 5000,
  message: "the clock to stay put while paused",
});
```

`waitFor` (`e2e/lib/proc.js:16-31`) ritorna appena la probe è truthy, e la prima probe gira immediatamente confrontando il valore con sé stesso: l'attesa termina in pochi millisecondi, il `timeout: 5000` non viene mai usato, e nessun intervallo viene mai osservato. Il commento sopra dice "mpv's own clock, held still. This is the proof the video is stopped, not the button": è un'intenzione, non una misura, e la regola del proprietario sui commenti la vieta.

La prima passata aveva scritto, di `n2-review-codice.md`: "`waitFor` a riga 140 ritorna al primo poll, perché `textContent` è sempre una stringa non vuota; il messaggio 'the transport label to settle' descrive un'attesa che non avviene mai". La correzione ha cambiato il selettore e ha rimesso lo stesso costrutto trenta righe più sotto.

Scenario concreto in cui morde, misurato e non ipotizzato: `.controls__time` ha risoluzione di **un secondo** (`VideoControls.tsx:6-11`, `formatTime` è `m:ss`), mentre l'intervallo fra la lettura di `frozen` (riga 175) e l'unica asserzione vera del caso (riga 196) è collapse + poll di `xwininfo` + restore + **un grab ffmpeg da 0.48 s** — misurato qui, quindi circa 0.6–1.0 s in totale, più lungo sul runner ma dello stesso ordine. Un video che stesse ancora andando mostrerebbe la stessa etichetta per tutta quella finestra ogni volta che non attraversa il confine del secondo. Il caso "in pausa" non ha quindi nessuna prova positiva di pausa presa da mpv: ha una etichetta scritta in modo ottimistico dal frontend (`useVideoPlayer.ts:92-96`) e un orologio la cui risoluzione è dello stesso ordine della finestra che deve misurare.

Correzione: la posizione grezza è già sullo schermo con precisione centesimale. `.controls__slider` è un `input[type=range]` con `step={0.01}` il cui `value` è `position` (`VideoControls.tsx:59-72`), cioè il float che arriva da `video://position`, a sua volta da `time-pos` osservata su mpv (`player.rs:193`). Leggere quello, campionarlo due volte a distanza esplicita (`sleep` reale di almeno 1.5 s fra i due campioni, oppure N campioni consecutivi tutti uguali su un intervallo dichiarato) e pretendere che non cambi. La stessa lettura va usata a riga 196 al posto dell'etichetta m:ss. E il commento va riscritto con il numero: quale risoluzione, su quale intervallo.

### [BLOCCANTE] src-tauri/src/video/mod.rs:29-32 + e2e/wdio.conf.js:15 — il ramo che la correzione ha introdotto (`VIDEO_OPEN == false`) non ha nessun test comportamentale, benché la prima passata l'avesse chiesto per nome

Il bloccante della prima passata (`n2-review-test.md`, terzo bloccante) chiudeva con: "Poi coprirlo: un caso E2E che all'avvio, senza aprire niente, verifica che la superficie sia `IsUnMapped` mentre `.stage__empty` è visibile". Il codice è arrivato, il test no. `EXPECTED_TESTS` passa da 27 a 30 e i tre test nuovi sono i tre di `video-surface.spec.js`, tutti dopo un `video_open` riuscito (`video-surface.spec.js:74-124` apre il fixture nel `before`). Nessuno spec esercita lo stato "nessun video aperto": `video.spec.js:85` guarda la superficie solo dopo l'apertura, e la sua unica asserzione è `IsViewable`.

Ho verificato a mano che oggi il codice è giusto: all'avvio la superficie è `IsUnMapped` (misura in cima a questo rapporto), quindi il difetto è la copertura, non il comportamento. Ma la copertura è esattamente il punto: `apply_region` ora contiene una macchina a stati a due ingressi (`region.is_empty()` e `VIDEO_OPEN`) di cui la suite esercita **una sola combinazione**. Scenario concreto: il prossimo che semplifica `apply_region` — per esempio riportando `set_shown(surface, true)` sul ramo non vuoto perché "tanto la regione arriva solo quando c'è un video" — ottiene una suite completamente verde e riporta in vita lo slab opaco sopra lo stage vuoto, che è il bloccante numero tre della prima passata. Su Windows lo slab è per giunta opaco davvero (`STATIC` dipinge il proprio sfondo), e la E2E non gira su Windows.

Correzione: un `it` in `video.spec.js`, prima di `opens the sample fixture`, il cui `before` non apre niente: attendere `.stage__empty` presente, trovare il figlio del toplevel con la geometria dello stage e pretendere `IsUnMapped`. Costa quattro righe, riusa i helper già importati in quello spec, e porta `EXPECTED_TESTS` a 31. In aggiunta, il caso simmetrico dell'errore (`video_open` fallito → superficie di nuovo nascosta, `mod.rs:138-144`) resta scoperto: `SUBLORE_E2E` ha già il percorso "file inesistente" nel banner d'errore, quindi il test è altrettanto economico.

---

## Seri

### [SERIO] e2e/specs/video-surface.spec.js:24-30 — il commento della soglia riporta una misura presa in uno stato che il test non misura mai, e sovrastima il margine di 130 volte

Il commento dice: "the empty stage reads 0.005 and the colour bars read 42.6, so anything between them separates the two states by four orders of magnitude". Lo 0.005 è verificato (ho misurato 0.00490) ma è lo stage **prima che un video sia mai stato aperto**, cioè uno stato in cui il test non si trova mai: i suoi tre casi vivono tutti dopo `video_open`. Il pavimento che conta è quello dello stage con il video caricato e la superficie smappata, e **quello vale 0.641509**, misurato qui con lo stesso comando. Il rapporto reale è 66:1, non 8500:1, e contro il chrome dell'app (1.1–2.0) è 21:1. Restano margini onesti, ma il numero scritto nel commento non descrive l'esperimento che il test esegue, e la regola del proprietario è che i commenti riportino misure.

Scenario in cui morde: M2.0 porta una banda di trascrizione e dei layer sopra lo stage (`docs/design/shell-layout.md:127-141` in questa stessa diff). Il giorno in cui qualcosa di colorato viene dipinto nel rettangolo dello stage — una barra di avanzamento, un banner d'errore rosso, un poster — chi legge quel commento crede di avere quattro ordini di grandezza di margine e ne ha meno di due.

Correzione, che chiude anche la domanda 4 del proprietario in modo indipendente dalla piattaforma: **calibrare dentro la corsa**. Lo stato nascosto il test lo produce già e lo aspetta già (`:139`, `:182`, `:202`); basta misurare lì il pavimento reale su quella macchina e pretendere `mostrato > max(PICTURE, 20 * pavimento)`, più `pavimento < PICTURE` come asserzione a sé. Due misure in più per caso, ~1 s, e la soglia smette di essere un numero importato da Fedora. In più, misurare il pavimento è l'unica prova positiva che il nascondimento ha davvero cancellato i pixel: oggi nessuna asserzione lo verifica, e la sequenza "nascondi, mostra, misura" non distingue un frame ridisegnato da pixel rimasti sullo schermo. (Su questa macchina si cancellano: 0.64. Sul runner, non verificato.)

### [SERIO] e2e/lib/pixels.js:64-71 — lo stato d'uscita e lo stderr di ffmpeg vengono buttati, e sulla piattaforma dove questo codice non ha mai girato non resta niente da leggere

`spawnSync` viene controllato solo per `run.error` (processo non avviabile o timeout). Se ffmpeg parte e fallisce — display sbagliato, `BadMatch` perché il rettangolo esce dallo schermo, `Cannot open display`, x11grab non compilato — `run.status` è diverso da zero, lo stderr contiene la causa esatta, e la funzione lancia `ffmpeg printed no signalstats saturation for {...}`, che non dice niente. Peggiora perché tutte le chiamate vivono dentro `waitFor`: l'errore viene catturato, ripetuto per 15 s e infine compresso nel suffisso `last error:` di un messaggio che accusa mpv ("a picture on the surface before the tests begin"). È la CI di Ubuntu, cioè l'unico posto dove questo codice non è mai stato eseguito, a pagare il conto.

Correzione: controllare `run.status !== 0` e includere le ultime righe di stderr nel messaggio; e, quando la misura fallisce, salvare il frame catturato come PNG accanto ai log (`-y out.png` invece di `-f null -`), perché su una macchina in cui nessuno può entrare la differenza fra diagnosi e congettura è quel file. Il job e2e non ha nessun `upload-artifact`: aggiungerne uno `if: failure()` è la metà mancante.

### [SERIO] docs/design/shell-layout.md:151 — il documento di design spedito in questa stessa diff afferma il contrario di quello che il codice di questa stessa diff fa

Il documento scrive: "`apply_region` does read an empty region as hide, but `set_region` never maps a hidden window, so sending a rectangle as the way back would move an unmapped window and show nothing". Dopo la correzione questo è falso: `apply_region` su regione non vuota chiama `set_region` **e poi** `set_shown(surface, VIDEO_OPEN)` (`mod.rs:229-230`), quindi mandare un rettangolo è esattamente la via del ritorno — ed è la via che i tre test nuovi usano per rimostrare la superficie (`video-surface.spec.js:144`, `:187`, `:206`). Il test dimostra il documento falso, nella stessa consegna.

Scenario: M2.0 implementa la decisione 1 su quella frase e sceglie una seconda strada per la visibilità, oppure il contrario — qualcuno legge il documento, conclude che il ritorno via regione non funziona e "aggiusta" `apply_region` togliendo `set_shown`, e i tre test nuovi diventano rossi per un motivo che il documento gli ha suggerito. CLAUDE.md §6 chiede che cambiare un'interfaccia significhi aggiornare i consumatori nella stessa consegna; qui il consumatore è il documento di design della milestone successiva.

Correzione: riscrivere il paragrafo con la semantica vera (regione vuota = nascondi; regione reale = posiziona e mostra se un video è aperto) e dire esplicitamente se la decisione 1 vuole un comando di visibilità separato o se riusa la regione. Nello stesso passaggio vanno corrette le citazioni ormai stantie che la diff stessa ha invalidato: `video/mod.rs:106` (righe 147 e 149) oggi è `take_surface`, `video/mod.rs:196-197` (riga 147) oggi è `with_surface`.

### [SERIO] e2e/specs/video-surface.spec.js:56-63 e :215-217 — la superficie viene identificata come "il figlio più grande sopra i 50 px" e il controllo anti-leak codifica la forma dell'albero X di questa macchina

`video.spec.js:114-133` identifica la superficie confrontando la geometria X con il rettangolo DOM entro 2 px. Lo spec nuovo abbandona quel metodo per un ordinamento per area, e poi asserisce che i figli sopra i 50 px sono esattamente uno. Ho misurato che su questa macchina è vero: il toplevel ha un solo figlio grande, perché WebKitGTK non crea una finestra X propria. È una proprietà del build di WebKitGTK, non della nostra applicazione, ed è precisamente il tipo di assunzione che CLAUDE.md §5.5 vieta: sul runner Ubuntu gira `webkit2gtk-driver` e un webkit diverso, e se quel build creasse una finestra nativa per la vista web, `surfaceWindow` restituirebbe **quella** (è più grande), il `before` la accetterebbe (ha figli), e i tre test fallirebbero accusando la superficie video di non nascondersi.

Il fallimento è rosso, non verde falso, ma è un rosso che accusa il codice sbagliato, e il repo aveva già il metodo giusto scritto e funzionante.

Correzione: riusare l'identificazione per geometria di `video.spec.js` (estrarla in `e2e/lib/x11.js` se serve a entrambi), e trasformare il controllo anti-leak in "nessun figlio _oltre_ quello identificato che abbia la geometria dello stage", invece di un conteggio totale.

---

## Minori

### [MINORE] e2e/specs/video-surface.spec.js:104-109 — il controllo su `.app__error` è letto troppo presto per poter scattare

Il banner viene letto nell'istante subito dopo il clic su Open, senza nessuna attesa: `video_open` non ha ancora avuto tempo di fallire, quindi la lettura è quasi sempre `null` e l'asserzione è decorativa. Un fixture illeggibile si manifesta 30 s dopo come "the surface with mpv attached inside it" scaduto. Correzione: mettere il controllo _dentro_ la probe del `waitFor` successivo, lanciando subito se il banner compare — così l'errore vero arriva in un secondo e con il testo giusto.

### [MINORE] e2e/specs/video-surface.spec.js:126-128 — il `before` non aspetta che il pulsante di trasporto sia abilitato prima che il primo test lo clicchi

`video.spec.js:70-76` aspetta `document.querySelector(".controls__button")?.disabled === false` prima di considerare pronto il video; qui il `before` si ferma alla presenza dei pixel, che arrivano quando mpv si aggancia, cioè potenzialmente prima che `video_open` sia ritornato al frontend e abbia messo lo stato a `ready`. Un clic su un pulsante ancora `disabled` non fa niente e il test fallisce 15 s dopo lamentando l'orologio fermo. Correzione: aggiungere la stessa attesa che l'altro spec ha già.

### [MINORE] e2e/lib/pixels.js:22-31 — `requireFfmpeg` sta nel `before` dello spec, non fra i prerequisiti di `onPrepare`

`wdio.conf.js:44-51` verifica display, binario e fixture prima della prima sessione, apposta. La prima passata aveva chiesto lo stesso trattamento per lo strumento di misura ("chiamato da `onPrepare`, così un binario mancante fallisce prima della prima sessione"). Così com'è, un ffmpeg mancante lascia partire l'app e quattro spec, poi fallisce dentro un hook e fa scattare anche la guardia `EXPECTED_TESTS`, sommando due messaggi che parlano di cose diverse. Correzione: chiamarlo in `onPrepare` accanto a `requireDisplay()`.

### [MINORE] e2e/README.md:7 e :77 — ffmpeg è dichiarato in una delle due liste di prerequisiti e non nell'altra

La riga 7 (nuova) elenca `xdotool`, `xwininfo`, python-xlib e `ffmpeg`; la lista canonica di riga 77, quella sotto "Prerequisites", continua a dire `xwininfo`, `xdotool`, `Xvfb`, python-xlib. Chi prepara una macchina legge la seconda. Correzione: una sola lista, o la seconda aggiornata.

### [MINORE] src-tauri/src/video/mod.rs:29-31 e :125-131 — il commento dichiara un'invariante che `video_open` non rispetta

Il commento dice che la visibilità segue `VIDEO_OPEN` "and never the geometry", ma l'invariante vera è `VIDEO_OPEN && !regione_vuota`: il modulo memorizza solo il primo dei due ingressi, e `video_open` mostra la superficie senza sapere se l'ultima regione era vuota. Se un `video_open` arriva mentre la regione è vuota, la superficie viene mappata sull'ultima geometria buona sopra uno stage che non la vuole. Oggi è latente (solo il test produce una regione vuota) e `shell-layout.md:149` dichiara che la shell di M2.0 rimetterà a posto lo stato dopo ogni comando, chiamando quel `show()` "a belt, not the design". Correzione minima: allineare il commento all'invariante reale; correzione completa: tenere anche l'ultima regione (o un flag `REGION_EMPTY`) e derivare la visibilità dai due ingressi in un unico punto.

### [MINORE] e2e/lib/pixels.js:44-63 — il comando ffmpeg dipende da due default impliciti

Manca `-draw_mouse 0`: x11grab disegna il puntatore dentro il frame catturato (default 1), quindi la misura include un cursore la cui posizione dipende dall'ultimo clic del test. Oggi è innocuo — il puntatore resta sui controlli, fuori dal rettangolo — ma è rumore gratuito in una misura di precisione. E manca `-loglevel info` esplicito: `metadata=print` scrive a livello info, quindi il parse dipende dal default di ffmpeg restando quello. Correzione: entrambi espliciti, una parola ciascuno.

### [MINORE] e2e/specs/video-surface.spec.js:113-124 — il rettangolo misurato viene catturato una volta nel `before` e mai più riletto

`surface` conserva la geometria letta all'apertura e ogni `saturation(surface)` successiva la riusa. Oggi è corretto, perché una regione vuota non chiama `set_region` (`mod.rs:226-228`) e il ripristino rimanda esattamente lo stesso rettangolo: l'ho verificato, 736x159+288+296 prima e dopo. Ma nulla nel test lo garantisce, e un futuro caso che ridimensiona la finestra misurerebbe in silenzio il posto sbagliato — e misurare il posto sbagliato dà 1.1–2.0, cioè sotto soglia, quindi rosso: sbagliato ma almeno non verde falso. Correzione: rileggere la geometria del figlio prima di ogni misura, o asserire che non è cambiata.

---

## Cose che ho guardato e su cui non ho rilievi

- **`set_shown` e lo stato iniziale `SHOWN = false`.** Nessun rilievo, e non per fiducia: su Linux `gdk::Window::new` restituisce una finestra non mappata e su Windows `CreateWindowExW` è chiamata senza `WS_VISIBLE` (`surface/windows.rs:34`), quindi `SHOWN = false` corrisponde alla realtà su entrambe le piattaforme al primo `apply_region`. Misurato su Linux: `IsUnMapped` all'avvio. Se una delle due `create` mappasse la finestra, la guardia di transizione la lascerebbe visibile per sempre; non è il caso.
- **`SHOWN` che diventa stantio.** Nessun rilievo: la superficie viene creata una volta in `setup` e distrutta solo in `shutdown`, non esiste un comando `video_close`, e `show`/`hide` non hanno altri chiamanti (verificato per grep sull'intero `src-tauri`). Se un giorno arriva `video_close`, dovrà passare da `set_shown` o il flag mente.
- **Errore da `surface.show()`.** Nessun rilievo: `set_shown` aggiorna il flag solo dopo che la chiamata è riuscita, quindi un fallimento non lascia il flag disallineato.
- **Fine del fixture durante la corsa.** Nessun rilievo: mpv gira con `keep-open=yes` (`player.rs:48`), quindi arrivare a 60 s tiene l'ultimo frame invece di svuotare la superficie, e il test dei dieci cicli non può diventare rosso per questo.
- **Il video parte in pausa.** Nessun rilievo: `pause=yes` fra le opzioni iniziali (`player.rs:50`) e `set_property("pause", true)` prima di ogni `loadfile` (`player.rs:265`), quindi il clic del primo test avvia davvero la riproduzione e l'orologio che avanza è prova positiva. Questa metà del caso "in riproduzione" è a posto.
- **Sicurezza dati (CLAUDE.md §3).** Nessun rilievo: `pixels.js` scrive su `-f null -` e non tocca il filesystem, il fixture è letto e mai riscritto, `XDG_DATA_HOME` resta puntato su una temp dir. `appEnv` copia l'ambiente e cancella `WAYLAND_DISPLAY` su una copia, non su `process.env` del processo corrente.
- **Budget (CLAUDE.md §7).** Nessun rilievo: il costo aggiunto è ~0.5 s per misura (misurato), una decina di misure in tutto lo spec; i dieci cicli usano solo `xwininfo`.
- **`EXPECTED_TESTS`.** Nessun rilievo: 27 + 3 = 30, e il README è aggiornato con le tre righe di tabella.

---

## Verdetto

**RICHIEDI MODIFICHE.**

Sulla domanda che il proprietario ha messo per prima, la risposta è tranquillizzante e va detta con i numeri: la soglia non si rompe attraversando la piattaforma. Il filtro esiste su Ubuntu 24.04, la chiave si chiama come previsto, e sullo stesso input Ubuntu e Fedora danno la stessa cifra fino al terzo decimale; la colorimetria muove il valore del 17%, non di un fattore 8. Quello che il commento dichiara male è il margine: 66:1 contro il pavimento vero, non 8500:1, perché lo 0.005 citato viene da uno stato che il test non attraversa mai. E resta un pezzo che nessuno può verificare da qui — se mpv disegni davvero sotto Xvfb sul runner Ubuntu — che però fallisce in rosso, non in verde.

I due bloccanti non riguardano la soglia. Uno è l'attesa vuota di riga 176, che è il difetto già bloccato nella prima passata riscritto dentro la sua stessa correzione, con un commento che promette una prova che non esiste. L'altro è il ramo `VIDEO_OPEN == false`: la correzione del bloccante più grave della prima passata è arrivata senza il test che quella stessa passata aveva chiesto, e oggi la macchina a stati nuova ha una sola delle sue combinazioni coperta.
