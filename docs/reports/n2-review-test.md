# Review N2 — il test della superficie video

Lente: il test nuovo (`e2e/specs/video-surface.spec.js`), letto con la regola del proprietario del 2026-08-29: un test comportamentale deve produrre prova positiva che l'azione è avvenuta, non solo assenza di disastro.

Diffe esaminata con `git diff` più i file non tracciati di `git status`: `src-tauri/src/video/mod.rs`, `e2e/wdio.conf.js`, `e2e/scripts/shutdown-check.js`, `e2e/scripts/close-gate-check.js`, `e2e/lib/env.js` (nuovo), `e2e/specs/video-surface.spec.js` (nuovo), `docs/design/shell-layout.md`, più i documenti M2.0 e `docs/reports/n2-probe.md`.

Cosa ho eseguito: `pnpm lint` (pulito), lettura del codice sorgente coinvolto, ispezione di `.github/workflows/ci.yml` e di ImageMagick sulla macchina locale. Non ho eseguito la suite E2E né ho provato niente su Windows. Ogni rilievo che dipende da un comportamento che non ho osservato lo dice esplicitamente.

---

## Bloccanti

### [BLOCCANTE] .github/workflows/ci.yml:135 — ImageMagick non esiste sul runner, e su Ubuntu il binario `magick` non esiste comunque

Il test misura i pixel con `execFileSync("import", ...)` e `execFileSync("magick", ...)` (`e2e/specs/video-surface.spec.js:33` e `:36`), ma la lista `apt-get install` del job `e2e smoke (ubuntu)` non contiene `imagemagick`. Nessuno dei due binari è presente sul runner.

C'è un secondo strato peggiore del primo. Anche aggiungendo `imagemagick` alla lista, `ubuntu-latest` è 24.04 e il pacchetto lì è ImageMagick 6, che fornisce `import`, `convert` e `identify` ma **non** `magick`: il binario unificato `magick` arriva solo con ImageMagick 7. Questa macchina è Fedora con ImageMagick 7.1.2, dove `magick` c'è, ed è esattamente il "funziona sulla mia piattaforma" che CLAUDE.md §5.5 vieta.

Scenario: si fa merge, CI gira, i tre test di `video-surface.spec.js` falliscono tutti su `spawn magick ENOENT` dentro il `before`. Il messaggio che arriva è "timed out after 15000ms waiting for a picture on the surface before the tests begin", con la causa vera relegata al suffisso `last error:`. Chi legge la CI rossa alle tre di notte pensa a mpv, non a un pacchetto mancante. In più `EXPECTED_TESTS = 30` non viene raggiunto, quindi anche la guardia anti-zero-test spara, sommando rumore al rumore.

C'è anche una violazione di CLAUDE.md §8 sopra a questa: ImageMagick è una dipendenza nuova dell'harness e non è dichiarata da nessuna parte. Non è nell'elenco prerequisiti di `e2e/README.md:68` (`xwininfo`, `xdotool`, `Xvfb`, python-xlib), e il progetto stesso si era già dato la regola in `docs/design/m2-0-tasks.md:403`: qualunque strumento di cattura schermo, e `import` di ImageMagick è nominato per nome, va dichiarato con licenza, compatibilità GPL e motivo per cui `xwininfo` non basta, e aggiunto alla lista dei prerequisiti.

Correzione: aggiungere `imagemagick` alla lista apt di `.github/workflows/ci.yml`, e non chiamare `magick` ma il binario che esiste su entrambe le distribuzioni. `convert` c'è in IM6 e in IM7 (in IM7 come alias deprecato), oppure si sceglie `identify -format` che è stabile su entrambe; in alternativa si pinna ImageMagick 7 esplicitamente e si documenta il perché. In parallelo: aggiungere un `requireImageMagick()` accanto a `requireDisplay()` in `e2e/lib/paths.js`, chiamato da `onPrepare`, così un binario mancante fallisce prima della prima sessione con il comando da eseguire, esattamente come già fanno `requireAppBinary` e `requireVideoFixture`. E aggiornare i prerequisiti di `e2e/README.md` con licenza e compatibilità GPL.

### [BLOCCANTE] e2e/specs/video-surface.spec.js:137-164 — il caso "in pausa" non prova niente su mpv, e non prova nemmeno che il video fosse in pausa

Il test si chiama "brings the picture back with the video paused, without restarting playback". La seconda metà del titolo è affidata a due letture della stessa etichetta:

```js
const label = await waitFor(() => browser.execute(() => document.querySelector(".controls__button")?.textContent ?? null), ...);
// ... hide, show ...
const stillPaused = await browser.execute(() => document.querySelector(".controls__button")?.textContent ?? null);
expect(stillPaused).toBe(label);
```

Due problemi distinti, ed entrambi mordono.

Primo, l'etichetta è stato del frontend, come sospettato. `VideoControls.tsx:54` stampa `paused ? play : pause` da `state.paused`, e `useVideoPlayer.ts:92-96` scrive quel campo in modo ottimistico subito dopo l'`invoke`, prima e indipendentemente da qualunque conferma di mpv. Un evento `video://state` da mpv la corregge dopo (`player.rs:502-508` osserva davvero la property `pause`), ma il test non aspetta quell'evento e non lo distingue dalla scrittura ottimistica. Se la riproduzione ripartisse per conto suo dentro mpv senza passare per il toggle, l'etichetta resterebbe identica e il test resterebbe verde.

Secondo, e più grave: l'asserzione non ancora mai l'etichetta a un valore. Confronta `label` con sé stesso letto più tardi. Scenario concreto in cui muore: il click su `.controls__button` a riga 139 non arriva, perché le coordinate sono sbagliate di qualche pixel o perché il bottone è ancora `disabled`. Allora il video non è mai stato messo in pausa, `label` vale "Pause" (o "Play", a seconda di cosa ha lasciato il test precedente), `stillPaused` vale lo stesso, e il test passa dichiarando un caso che non ha mai costruito. Il tipo esatto di asserzione che il template di review (`WORKFLOW.md` §4b) cita come già trovata in questo repo.

La prova positiva esiste ed è a portata di mano. `position` in `useVideoPlayer.ts:56-58` arriva da `video://position`, che il backend emette da `time-pos` osservato su mpv (`player.rs:193` e `:138`). È il canale onesto, non uno stato React. E la fixture è `testsrc2` (`fixtures/video/make-sample.sh:5`), quindi animata: due frame consecutivi differiscono sempre.

Correzione: asserire su `.controls__time` (o sul valore di `.controls__slider`), non sull'etichetta. Leggere la posizione dopo la pausa, verificare che sia ferma per un intervallo misurabile prima di nascondere, e verificare che sia ancora la stessa dopo il ri-show. In più, ancorare l'etichetta al valore atteso (`expect(label).toBe(en.video.play)`), così un click perso fallisce sul click invece di passare inosservato. Meglio ancora, in aggiunta: `spread()` salva già i PNG, quindi confrontare byte a byte il ritaglio prima di nascondere e dopo il ri-show dà la prova diretta che è tornato lo _stesso_ frame e non un frame più avanti.

### [BLOCCANTE] src-tauri/src/video/mod.rs:196-206 — la correzione mostra la superficie anche quando nessun video è aperto, e nessun test copre quel caso

La superficie non nasce con il video: viene creata durante `setup`, all'avvio dell'app (`mod.rs:63-66`). `VideoStage` monta e chiama `schedule()` immediatamente (`VideoStage.tsx:44`), che manda un rettangolo reale e non vuoto. Prima della correzione quel rettangolo produceva solo `set_region`, e su Linux `set_region` è `move_resize` più `raise` senza alcuna mappatura (`surface/linux.rs:62-67`): la superficie restava non mappata e invisibile. Dopo la correzione ogni aggiornamento di regione non vuota chiama anche `show()`, quindi la superficie viene mostrata e alzata sopra la webview all'avvio, con nessun video caricato.

Su X11 questo probabilmente non si vede: `docs/reports/n2-probe.md:38` e :44 documentano che una superficie mappata senza mpv agganciato lascia vedere la webview sotto. Su Windows la storia cambia. La superficie è una finestra figlia di classe `STATIC` (`surface/windows.rs:32`), e uno `STATIC` con didascalia vuota dipinge il proprio sfondo con il pennello che il genitore restituisce a `WM_CTLCOLORSTATIC`. Il risultato atteso è un rettangolo opaco sopra lo stage all'avvio, che copre il placeholder "No video open." reso da `.stage__empty` (`VideoStage.tsx:59`, `App.css:112-121`).

Non l'ho verificato su Windows: è ragionamento sulla semantica della classe `STATIC`, non un'osservazione. Ma il punto sta in piedi comunque, perché è proprio la lente del test a scoprirlo: **il caso che la correzione ha cambiato di più non ha copertura**. Il nuovo spec prova solo scenari dopo `video_open`. La regione mandata a monte di qualunque video, che è il caso di gran lunga più frequente durante l'avvio, non è testata su nessuna piattaforma, e la E2E gira solo su ubuntu (`docs/design/post-v1-plan.md:263`), quindi non la coprirebbe nemmeno se il test ci fosse.

Lo stesso vale per il percorso di errore: `video_open` che fallisce chiama `surface.hide()` (`mod.rs:114-116`), ma il primo aggiornamento di regione successivo, per esempio un resize della finestra, rifà `show()` e annulla quel nascondimento.

Correzione: la superficie deve seguire la regione _e_ lo stato del player, non la sola regione. Tenere accanto alla `SURFACE` un flag "deve essere visibile", messo da `video_open` in caso di successo e tolto in caso di errore, e far chiamare `show()` da `apply_region` solo quando quel flag è alzato. Poi coprirlo: un caso E2E che all'avvio, senza aprire niente, verifica che la superficie sia `IsUnMapped` mentre `.stage__empty` è visibile.

---

## Seri

### [SERIO] e2e/specs/video-surface.spec.js:114-135 — il caso "video in riproduzione" non prova mai che il video stia andando

Il test clicca play a riga 116 e poi aspetta `spread(surface, "play-before") > PICTURE`. Ma un'immagine sulla superficie c'era già: il `before` a riga 108 ha aspettato esattamente la stessa condizione, sullo stesso rettangolo, con la stessa soglia. Il video aperto e in pausa al fotogramma 0 mostra già le barre di `testsrc2`.

Scenario: il click su `.controls__button` non arriva, o `video_play` fallisce e finisce nel banner d'errore che il test non guarda. Il video resta in pausa. Il test nasconde, rimostra, trova un'immagine, trova `IsViewable`, e passa dichiarando "with the video playing". Il caso in riproduzione, che è il motivo per cui il test esiste in due varianti, non è mai stato costruito. Vale la stessa regola della prova positiva del rilievo precedente: qui manca la prova che l'azione (avviare la riproduzione) sia avvenuta.

Correzione: dopo il click su play, aspettare che la posizione riportata da mpv sia avanzata rispetto a quella letta prima del click, e solo allora nascondere. Con la fixture animata funziona anche il confronto fra due ritagli catturati a distanza: se differiscono, sta disegnando frame nuovi.

### [SERIO] e2e/specs/video-surface.spec.js:92-97 — il setup scrive nel campo senza aspettare il fuoco, e il fallimento accusa il codice sbagliato

```js
const field = await centreOf(".bar__input");
clickAt(toplevel.absX + field.x, toplevel.absY + field.y);
execFileSync("xdotool", ["type", "--delay", "5", videoFixture], { timeout: 15000 });
const button = await centreOf(".bar__button");
clickAt(toplevel.absX + button.x, toplevel.absY + button.y);
```

`video.spec.js:53-64` fa la stessa sequenza con due attese in mezzo: aspetta che `document.activeElement.className === "bar__input"` e poi che il valore digitato sia davvero arrivato nel campo. Qui entrambe sono sparite, e non c'è nemmeno un controllo su `.app__error` dopo l'Open. Il commento di `video.spec.js:43` dice esplicitamente perché quelle attese ci sono.

Scenario: il fuoco non è ancora sul campo quando parte `xdotool type`. I caratteri finiscono nel vuoto, Open viene cliccato con il campo vuoto, l'app mostra un errore, nessun video si apre. Il test poi resta appeso trenta secondi e muore con "timed out after 30000ms waiting for the surface with mpv attached inside it", stampando l'albero X11 completo. La colpa finisce sulla superficie video e su mpv, quando la causa è una corsa nella digitazione dell'harness. È esattamente la modalità di fallimento che `n2-probe.md:40-46` racconta di aver già pagato due volte in questa area.

Correzione: riusare le stesse due attese di `video.spec.js`, e aggiungere subito dopo l'Open l'attesa già usata lì sullo stato pronto (`.stage__empty === null` e `.controls__button.disabled === false`) più il controllo che `.app__error` sia nullo. Così un'apertura fallita fallisce sull'apertura, con il messaggio dell'app, e non trenta secondi dopo su mpv.

### [SERIO] e2e/README.md:94 e :11-37 — la tabella delle spec e il numero atteso non sono stati aggiornati

`EXPECTED_TESTS` è passato da 27 a 30 in `wdio.conf.js:15`, e il numero è corretto: ho contato le `it` per file (asr 5, editor 10, project 5, subtitle 3, title 2, video 2, video-surface 3) e fanno esattamente 30. Ma `e2e/README.md:94` mostra ancora `const EXPECTED_TESTS = 27;` in un blocco di codice, e tre righe dopo il README stesso scrive in grassetto "Update the number when you add or remove a test". La tabella inventario delle spec (righe 11-37) elenca ogni singolo test di ogni spec con la sua descrizione e non contiene nessuna riga per `video-surface.spec.js`.

Scenario: il prossimo che aggiunge un test legge il README, vede 27, si fida del documento invece del codice, e scrive un numero sbagliato. Oppure guarda la tabella per capire cosa la suite copre già e conclude che la superficie non è coperta oltre al dimensionamento, riscrivendo un test che esiste.

Correzione: aggiornare il blocco a 30 e aggiungere le tre righe alla tabella, nello stesso formato delle altre.

---

## Minori

### [MINORE] e2e/specs/video-surface.spec.js:133 e :158 — due asserzioni su una costante

Contate, come richiesto: **due**. `expect(back).toBe(true)` a riga 133 e a riga 158. In entrambi i casi `back` è il ritorno di `waitFor`, la cui sonda restituisce letteralmente `true` o `null`, e `waitFor` ritorna solo su valore vero (`proc.js:20-22`). Se l'esecuzione arriva a quella riga, `back` non può essere altro che `true`. La verifica vera è il timeout del `waitFor` sopra.

Non sono pericolose: non nascondono niente, perché l'attesa che le precede è reale. Ma sono rumore che assomiglia a copertura, in un repo dove secondo `WORKFLOW.md` §4b una review precedente ha già trovato un contatore di asserzioni che proteggeva tre asserzioni che non asserivano nulla. Correzione: toglierle, oppure fare in modo che la sonda restituisca il valore misurato invece di `true`, così l'asserzione può dire qualcosa di reale (`expect(back).toBeGreaterThan(PICTURE)`).

### [MINORE] e2e/specs/video-surface.spec.js:26-27 — PICTURE = 0.05 è un numero preso da una macchina sola

La soglia viene dal probe: barre a colori 0.38, buco vuoto 0.00 (`n2-probe.md:9` e le tabelle). Il margine è ampio, ma il numero è scritto a mano e non calibrato a runtime.

Sul caso che la domanda solleva, il rendering software del runner che produce un'immagine più piatta: **non produce un falso verde**. Il `before` a riga 108 aspetta la stessa condizione con la stessa soglia sullo stesso rettangolo, quindi se su quella macchina un'immagine viva non arriva a 0.05 il setup fallisce e i tre test non partono. Il fallimento è rumoroso e nel posto giusto. Lo escludo come rilievo grave per questo motivo.

Resta però il verso opposto, non coperto: nessuno asserisce mai che lo stato nascosto stia _sotto_ la soglia. Il salvataggio qui è accidentale e non documentato, cioè `.stage` ha `background: #000` uniforme (`App.css:104`) e il placeholder `.stage__empty` non viene reso quando un video è aperto (`VideoStage.tsx:59`), quindi sotto la superficie non c'è niente che possa produrre varianza. Se domani qualcuno mette un poster, un bordo o un gradiente sotto lo stage, quell'invariante salta in silenzio e il ritaglio supera 0.05 senza che mpv disegni niente. Correzione: misurare `spread` anche mentre la superficie è nascosta e asserire che sia sotto la soglia. Costa una riga per test e trasforma un'invariante accidentale in un'asserzione.

### [MINORE] e2e/specs/video-surface.spec.js:154-158 — il caso in pausa non ricontrolla lo stato di mappatura

I test 1 (riga 134) e 3 (righe 174-177) verificano `IsViewable` dopo il ri-show. Il test 2 no: si ferma allo `spread`. Oggi non morde, per l'invariante del fondo nero appena descritta, ma è un'asimmetria gratuita fra tre casi che dovrebbero misurare la stessa cosa. Correzione: aggiungere `expect(mapState(surface.id)).toBe("IsViewable")` anche lì.

### [MINORE] e2e/specs/video-surface.spec.js:43-53 e :93-97 — `centreOf` duplicata da video.spec.js, e la guardia sul null è caduta per strada

`centreOf` è copiata parola per parola da `video.spec.js:13-23`. Ma `video.spec.js` la incapsula in `clickElement` (righe 25-32), che controlla il null e lancia `${selector} is missing from the DOM`. La versione nuova chiama `field.x` e `button.x` direttamente su un valore che può essere `null`.

Scenario: la UI non ha ancora reso `.bar__button` quando il setup lo cerca. Il test muore con `TypeError: Cannot read properties of null (reading 'x')` dentro un hook `before`, senza dire quale selettore mancava. Correzione: sollevare `centreOf` e `clickElement` in `e2e/lib/` e usarli da entrambe le spec. CLAUDE.md §6 chiede di riusare quello che esiste prima di scrivere, e qui la copia ha perso proprio il pezzo che rendeva l'originale diagnosticabile.

### [MINORE] e2e/specs/video-surface.spec.js:67-74 — `surfaceWindow` identifica la superficie con un criterio più debole di quello che il repo usa già

`video.spec.js:116-134` trova la superficie facendo combaciare la sua geometria con il rettangolo di `.stage__surface` entro 2 px. La nuova `surfaceWindow` prende invece il figlio diretto più grande sopra i 50x50 pixel. Se un giorno il toplevel avesse un secondo figlio grande, per esempio la finestra della webview su un altro build GTK, il setup si aggancerebbe a quella e ogni misura successiva sarebbe fatta sul posto sbagliato. Il fallimento non sarebbe silenzioso, perché l'attesa di `IsUnMapped` scadrebbe, ma il messaggio parlerebbe della superficie video mentre il problema è l'identificazione.

C'è anche una contraddizione interna: `surfaceWindow` è scritta per tollerare più figli grandi, li ordina per area e prende il primo, mentre il test 3 a riga 183 asserisce che di figli grandi ce n'è esattamente uno. O ne può esistere più d'uno, e allora l'asserzione del test 3 è fragile, o non può, e allora il filtro più l'ordinamento sono complessità morta che nasconde il criterio vero. Correzione: identificare la superficie dal rettangolo dello stage come fa già `video.spec.js`, e fallire con un messaggio esplicito se i candidati non sono esattamente uno.

### [MINORE] e2e/specs/video-surface.spec.js:101-107, 184 — il rettangolo della superficie è congelato al setup

`surface` viene catturata una volta nel `before` e le sue coordinate vengono riusate da `spread()` in tutti e tre i test. Il test 3 rilegge i figli a riga 182 ma continua a misurare il rettangolo vecchio. Se il layout si sposta durante la corsa, per esempio perché compare il banner `.app__error` che alza tutto di qualche riga, il ritaglio misura l'area sbagliata. Con `testsrc2` a schermo la misura resta sopra soglia comunque, quindi non è un falso rosso, è un'asserzione che ha smesso di dire dove si trova la superficie. Correzione: rileggere la geometria della superficie prima di ogni `spread`, o almeno asserire che non sia cambiata.

### [MINORE] e2e/specs/video-surface.spec.js:166-185 — il caso dei dieci cicli: nessun rilievo sui due punti chiesti, uno sul terzo

Sui due punti della domanda il test è a posto e lo escludo esplicitamente. Asserisce che l'immagine è ancora viva alla fine: riga 184, `expect(spread(surface, "cycles-after")).toBeGreaterThan(PICTURE)`, ed è una misura vera sui pixel, non un `true`. E un ciclo che fallisse a metà se ne accorgerebbe: ogni attesa dentro il ciclo porta il numero del ciclo nel messaggio (righe 172 e 176), quindi il fallimento dice "the surface to hide on cycle 6", non un timeout anonimo a fine test.

Quello che manca è la parte di dentro. Il ciclo verifica solo `IsUnMapped` e `IsViewable`, mai i pixel, quindi il test dimostra che l'immagine è viva al ciclo 0 e al ciclo 9 ma non che lo sia stata nel mezzo. Una regressione che degrada il ri-show progressivamente, per esempio mpv che smette di ridisegnare dopo qualche remap ma la finestra resta mappata, passerebbe tutti e dieci i cicli e verrebbe presa solo per caso dalla misura finale. Correzione: misurare `spread` almeno a metà corsa, o a ogni ciclo se il costo lo permette.

### [MINORE] e2e/specs/video-surface.spec.js:28 — la directory degli screenshot non viene mai rimossa

`mkdtempSync` crea `n2-spec-XXXX` sotto la temp e nessuno la cancella. Dentro finiscono PNG dell'intera finestra radice, uno per tag, riscritti a ogni sondaggio. Non è un problema di sicurezza dati (CLAUDE.md §3 riguarda i file dell'utente, e qui siamo nella temp), è sporcizia che si accumula a ogni corsa. Nota di contorno: durante un'attesa fallita il polling scatta ogni 250 ms per 15 secondi, quindi fino a sessanta catture a schermo intero più sessanta invocazioni di ImageMagick per singola attesa. Correzione: rimuovere la directory in un `after`, tenendo i file quando il test è fallito se servono per la diagnosi.

### [MINORE] e2e/specs/video-surface.spec.js:56-65 — nessun ripristino dello stile dopo un fallimento

`setStageCollapsed` scrive `element.style.height = "0px"` direttamente nel DOM e non c'è nessun `after` o `afterEach` che lo rimetta a posto. Se un test fallisce fra il collasso e il ripristino, i test rimanenti del file partono da uno stage già collassato: il test successivo aspetta "the surface to hide" su una superficie già nascosta e scade con un messaggio fuorviante. Un fallimento diventa tre. Correzione: un `afterEach` che riporta `style.height` a stringa vuota.

Nota di contesto, non un rilievo: manipolare lo stile inline non è un'azione che il prodotto offre a un utente, quindi il commento di intestazione ("driven through the product's own path") è un po' generoso. Il percorso _del backend_ è quello vero, perché la regione passa davvero per `ResizeObserver`, IPC e `apply_region`. Ma l'innesco non lo è, e non lo può essere finché la decisione 1 non esiste. Va bene così, purché non venga descritto come una simulazione del gesto utente.

### [MINORE] src-tauri/src/video/mod.rs:203-204 — `raise` viene chiamata due volte per aggiornamento

`set_region` su Linux fa `move_resize` più `raise` (`surface/linux.rs:65-66`) e `show` fa `show` più `raise` di nuovo (`:70-73`). Dopo la correzione ogni aggiornamento di regione ne fa due di fila. Non tocca nessun budget di §7 in modo misurabile, ma `VideoStage` manda un aggiornamento per frame durante un resize, quindi è traffico X inutile su un percorso caldo. Correzione: se il flag di visibilità del rilievo bloccante 3 viene introdotto, `show()` va chiamata solo alla transizione, non a ogni regione, e il problema sparisce da sé.

### [MINORE] e2e/wdio.conf.js:20-23 contro e2e/lib/env.js:15-19 — la stessa intenzione scritta due volte in due modi

`appEnv()` costruisce l'ambiente per i processi che i due script di controllo lanciano direttamente, mentre `wdio.conf.js` muta `process.env` perché lì l'app la lancia tauri-driver, che eredita. Il meccanismo funziona, l'ho tracciato: `driver.js:70-82` spawna senza `env` esplicito, quindi eredita l'ambiente ripulito del worker. Ma sono due copie della stessa regola, e la seconda che dimenticherà di seguire la prima non fallirà, mostrerà solo di nuovo il sintomo che il probe ha impiegato due corse a diagnosticare. Correzione: far leggere a `wdio.conf.js` la stessa fonte, per esempio esportando da `env.js` la lista delle variabili da mettere e da togliere e applicandola in entrambi i posti.

### [MINORE] git status — la diffe mescola N2 con documenti M2.0 non correlati

L'albero di lavoro porta anche `docs/design/m2-0-tasks.md`, `docs/reports/m2-0-critique-incrementalita.md` e `docs/reports/m2-0-critique-osservabilita.md`, che con N2 non c'entrano. `WORKFLOW.md` §4 chiede consegne grandi quanto una sessione di revisione sola. Correzione: separarli in una consegna a sé prima del merge locale.

---

## Cose che ho controllato e che sono a posto

**La correzione in `apply_region` cattura davvero la regressione che dice di catturare.** Non è scontato, quindi l'ho verificato invece di assumerlo. `set_region` su Linux è `move_resize` più `raise` e non mappa niente (`surface/linux.rs:62-67`). Se si togliesse `surface.show()`, dopo `setStageCollapsed(false)` la superficie resterebbe non mappata, il ritaglio misurerebbe il nero uniforme di `.stage` e darebbe circa 0, l'attesa a riga 129 scadrebbe e il test 1 fallirebbe, seguito dal test 3 sull'attesa di `IsViewable`. Il test è una guardia vera contro esattamente il difetto che N2 corregge. È la cosa migliore di questa consegna.

**Il conteggio `EXPECTED_TESTS = 30` è corretto rispetto a quello che le spec dichiarano.** Contato per file, nessuna `it` annidata o scritta in forma diversa che sfugga al conteggio. Il difetto è solo nel README, sopra.

**La diagnosi su Wayland in `env.js` è giusta e il rimedio arriva dove serve.** La motivazione è documentata sia nel modulo sia in `n2-probe.md:44`, i due script di controllo la usano, e la catena di ereditarietà fino all'app lanciata da tauri-driver regge. È l'opposto di un cambiamento fatto a caso, e trasforma in costruzione quello che il probe descriveva come fortuna.

**Nessun `unwrap()` introdotto, nessun segreto, nessun codice dei moduli chiusi, nessuna scrittura sui file dell'utente.** Il test scrive solo nella propria directory temporanea e apre la fixture in sola lettura. `pnpm lint` passa pulito. I commenti nuovi in `mod.rs` stanno nelle due righe che CLAUDE.md §6 concede.

---

## Verdetto

**RICHIEDI MODIFICHE**

Tre bloccanti. Il primo rende il test impossibile da eseguire sul solo runner che la CI ha, e lo fa in due modi indipendenti (pacchetto assente, e binario che su Ubuntu non esiste comunque). Il secondo lascia il caso in pausa, cioè il caso che il probe indica come quello che l'utente incontra davvero, poggiato su un confronto dell'etichetta con sé stessa: viola frontalmente la regola della prova positiva, e la prova positiva era disponibile a due righe di distanza nella posizione riportata da mpv. Il terzo è nella correzione stessa, che mostra la superficie in uno stato in cui prima restava nascosta e che nessun test copre su nessuna piattaforma.

Il nucleo della consegna è sano: la correzione a `apply_region` è quella giusta per il difetto giusto, il test la sorveglia davvero, e il lavoro sull'ambiente Wayland chiude un buco reale nell'harness. Quello che manca è la disciplina della prova positiva sui due casi che il test _dichiara_ di distinguere, e l'onestà sulla piattaforma su un cambiamento che tocca entrambi i backend di superficie.
