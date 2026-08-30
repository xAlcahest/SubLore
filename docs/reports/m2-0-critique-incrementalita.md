# M2.0 — critica avversariale, lente: incrementalità

Verifica di `docs/design/m2-0-tasks.md` contro il codice, non contro la memoria. Piattaforma: Linux.
Nessun file toccato oltre questo.

Fonti lette: `CLAUDE.md` §1 §3 §5 §6 §7 §9; `docs/design/shell-layout.md`;
`docs/design/decisions.md`; `BACKLOG.md` M2.0 (sola lettura); `docs/reports/n2-probe.md`;
`WORKFLOW.md`; `src/App.tsx`, `src/App.css`, `src/components/*.tsx`, `src/hooks/*.ts`,
`src/i18n/en.ts`; `src-tauri/src/lib.rs` e `src-tauri/src/subtitle/mod.rs`; tutte e sei le spec in
`e2e/specs/`, `e2e/wdio.conf.js`, `e2e/scripts/close-gate-check.js`, `e2e/scripts/shutdown-check.js`,
`.github/workflows/ci.yml`.

La domanda a cui rispondo è una sola: **ogni task, se il lavoro si ferma lì, lascia un'app che
l'utente apre e usa e una batteria verde?** Il piano lo afferma in §5 ("Every merged task leaves a
working, verifiable app"). Non è vero per almeno cinque punti.

---

## Premessa che genera metà dei rilievi

`e2e/scripts/close-gate-check.js` non ha una sessione WebDriver e non ha DOM. Raggiunge il documento
con tre punti in pixel assoluti, dichiarati fragili dal file stesso:

```js
/** Points in the current shell, relative to the toplevel origin. M2.0 must revisit these. */
const SUBTITLE_PATH_FIELD = { x: 506, y: 73 };
const SUBTITLE_OPEN_BUTTON = { x: 676, y: 73 };
const FIRST_CUE_TEXT = { x: 750, y: 540 };
```

Le sue 12 verifiche girano in CI a ogni push (`.github/workflows/ci.yml`, step "Close gate test"),
e coprono l'unico difetto di perdita dati che il progetto ha già trovato e chiuso (decisione 9).
Il piano assegna la revisione di quei punti **solo a T5b**. Ma la colonna verticale del workspace,
da cui quei punti dipendono, viene mossa da T2, T3 e T5a. Nessuno di quei tre task possiede il file.
Un implementer di T3 che trova il close gate rosso deve toccare un file fuori dalla sua ownership,
cioè fermarsi con un BLOCKED o driftare (WORKFLOW §4, §5).

Aritmetica che serve sotto: `ROW_HEIGHT = 28` (`CueList.tsx:17`), la fixture del close gate
(`fixtures/subtitles/srt/clean/basic-lf.srt`) ha **3 cue**, quindi le righe cliccabili occupano
`3 × 28 = 84 px` in fondo a un pannello alto ~217 px. Sotto gli 84 px c'è vuoto: un click lì non
apre niente.

---

## BLOCCANTI

### B1 — T3 sposta la griglia e rompe il close gate, due task prima che qualcuno lo ricalcoli

**Task:** T3.

**Difetto:** T3 toglie dallo schermo la banda di trascrizione, la colonna del workspace si accorcia,
la griglia risale, e `FIRST_CUE_TEXT = {750, 540}` finisce plausibilmente sul vuoto sotto le tre
righe della fixture, ma la ricomputazione di quel punto è assegnata a T5b.

**Perché.** Oggi la banda ASR è `flex: none` (`App.css:395-403`) più la sua status line
(`App.css:429-440`): circa 75 px di colonna. `.stage` è `flex: 1 1 45%` e `.cuelist__panel` è
`flex: 1 1 55%` (`App.css:100`, `App.css:209`), quindi i 75 px liberati vanno per il 55% alla
griglia: il bordo superiore della lista sale di circa 40 px. Le righe coprono 84 px. Perché il
click continui a cadere su una riga dopo uno scorrimento di ~40 px verso l'alto, oggi deve cadere
nei primi ~43 px della banda di righe, cioè sulla riga 1 o sulla metà alta della riga 2. Non è
misurato da nessuno, e il margine è meno di due righe.

**Cosa vede l'utente che si ferma a T3:** l'app parte, apre file, riproduce video. Ma la CI è rossa
e il messaggio che riceve è fuorviante: `waitForDialog` lancia "the app exited (code 0) instead of
asking", cioè accusa il close gate di aver lasciato chiudere un documento sporco, quando in realtà
il documento non è mai stato sporcato. Il piano ha costruito quel messaggio proprio per distinguere
i due casi (`close-gate-check.js:81-95`) e questo scenario li rifonde.

**Correzione concreta.** Due parti, entrambe piccole.

1. Aggiungere `e2e/scripts/close-gate-check.js` ai file posseduti da **ogni** task che cambia lo
   stack verticale del workspace: T2, T3, T5a, T5b. Aggiungere a ciascuno l'AC esplicita
   "`pnpm e2e:close-gate` passa con 12/12 dopo la modifica".
2. Meglio ancora, togliere la sensibilità all'altezza di riga una volta per tutte, in T3, dove il
   problema si presenta per primo. `.cuelist` ha `tabIndex={0}` e `onListKeyDown` mappa Enter su
   `beginEdit(selected)` con `selected` inizializzato a 0 (`CueList.tsx:85`, `CueList.tsx:236-243`).
   Quindi `openAndDirty` può: cliccare **un punto qualsiasi dentro `.cuelist`** (bersaglio ~217 px
   invece di 28), premere Enter per aprire l'editor sulla riga 1, digitare, premere Return. Il punto
   resta uno solo, molto più tollerante, e il debito N1 sui `sleep` fissi non peggiora.
   Attenzione: se T4 sposta l'apertura dell'editor sul doppio click (vedi S4), questa rotta va
   verificata nello stesso task.

---

### B2 — T5a smonta il guscio vecchio e lascia il close gate senza coordinate fino a T5b

**Task:** T5a.

**Difetto:** T5a riscrive `App.css` e la composizione (rail a sinistra, banda alta con video e
colonna destra, griglia a tutta larghezza sotto, status line in fondo) senza possedere
`close-gate-check.js`, e il piano rimanda esplicitamente la ricomputazione a T5b
("`FIRST_CUE_TEXT` is recomputed for the new grid position at 1024x700", T5b).

**Perché.** T5a aggiunge una status line in fondo, che spinge la griglia in su, e cambia l'altezza
della banda alta (video più colonna destra) rispetto all'attuale `.stage` + `.controls`. Tutti e tre
i punti si spostano. In più le barre parcheggiate finiscono "in one strip above the top band", quindi
anche `SUBTITLE_PATH_FIELD` e `SUBTITLE_OPEN_BUTTON` cambiano riga. Questo è esattamente il punto che
la lente cerca: il guscio vecchio è smontato e la verifica che lo raggiungeva non è ancora
riagganciata. Il piano lo ammette per un punto su tre e lo rimanda di un task.

**Cosa vede l'utente che si ferma a T5a:** un'app funzionante e brutta (le barre parcheggiate), che
è il compromesso dichiarato e accettabile. Ma la batteria non è verde, e il gate che protegge dalla
perdita di lavoro non gira. Un merge in quello stato viola il vincolo che T5a stesso rivendica.

**Correzione concreta.** Spostare la ricomputazione dei tre punti da T5b a T5a, cioè aggiungere
`e2e/scripts/close-gate-check.js` ai file posseduti da T5a con l'AC "12/12 dopo il rifacimento del
layout"; T5b poi cambia solo la rotta di apertura (bottone della toolbar più helper T1), non i punti.
Se si applica la correzione 2 di B1, a T5a resta da ricalcolare un solo punto invece di tre.

---

### B3 — T3 si contraddice su `Transcribe…` senza video, e la prima verifica ASR non può più partire

**Task:** T3.

**Difetto:** T3 dichiara "A `Transcribe…` control is visible and **disabled** with no video open" e,
quattro righe più sotto, "With no video open, `Transcribe…` cannot be started, **and the dialog says
why**". Se il controllo è disabilitato il dialogo non si apre mai senza video, e la prima delle
cinque verifiche di `asr.spec.js` diventa irraggiungibile.

**Perché.** `asr.spec.js:123-146` ("offers the models it knows and a compute choice") gira **prima**
che un video sia aperto: è la verifica che asserisce il catalogo (`>= 10` opzioni, `tiny.en`
presente e selezionato, etichetta "ready"), l'assenza del bottone di download, la spunta GPU, lo
start disabilitato e la status line idle. Tutto tranne la status line vive, dopo T3, dentro il
dialogo (`.transcribe__model`, `.transcribe__gpu`, `.transcribe__start`, `.transcribe__download`).
Con il controllo disabilitato quelle sette asserzioni non hanno un DOM da interrogare, e l'unico
modo per farle passare è aprire un video prima, cioè cambiare la precondizione della verifica.
Questo non è "aggiungere un passo per raggiungere un controllo": è cambiare cosa la verifica
afferma, ed è §5.4.

**Correzione concreta.** Fissare la regola nel senso della seconda frase, che è anche quella che
riproduce il comportamento di oggi: `Transcribe…` **apre sempre** il dialogo; è `Start` dentro il
dialogo a essere disabilitato senza video, esattamente come `.asrbar__start` è `disabled === true`
oggi (`asr.spec.js:144`). Correggere il primo AC di T3 e aggiungere alla riga di riaggancio la nota
che la verifica 1 apre il dialogo per primo e mantiene le sette asserzioni alla lettera.

---

### B4 — T3 chiude il dialogo all'avvio della corsa ma lascia esiti ed errori dentro il dialogo chiuso

**Task:** T3.

**Difetto:** la mappa dei selettori mette `__error`, `__cue`, `__cue-time`, `__cue-text` nel gruppo
`.transcribe__…` "inside the dialog", e la §7 domanda 2 decide che l'avvio della corsa chiude il
dialogo. Tutto ciò che una corsa produce finisce quindi in un contenitore chiuso: invisibile
all'utente e irraggiungibile da due delle cinque verifiche ASR.

**Perché, tre istanze concrete.**

- `asr.spec.js:276-305` ("refuses a damaged model"): clicca start e poi aspetta
  `.asrbar__error` → `.transcribe__error`. La corsa fallisce sul checksum **dopo** che il dialogo si
  è chiuso, quindi il banner viene disegnato dentro il dialogo chiuso. La `waitFor` non si risolve
  mai. Peggio del test: l'utente clicca Start, il dialogo sparisce e non gli viene detto niente. È
  un errore inghiottito, CLAUDE.md §6.
- `asr.spec.js:175-183`: `shownCues()` legge `.asrbar__cue` → `.transcribe__cue` **dopo** la fine
  della corsa, e confronta il numero di cue mostrate con quello sulla status line
  (`expect(status).toContain(\`${cues.length} cues\`)`). Con il dialogo chiuso l'array è vuoto e
`expect(cues.length).toBeGreaterThan(0)` cade.
- `asr.spec.js:253` ("Back to a usable state"): dopo il cancel asserisce
  `propertyOf(".asrbar__start", "disabled") === false`. Con `.transcribe__start` in un dialogo
  chiuso, `propertyOf` restituisce `null`, non `false`, e `toBe(false)` fallisce.

**Correzione concreta.** Spostare la cucitura. Nel dialogo restano solo gli **ingressi** della
corsa: catalogo modelli, spunta GPU, download, start. Tutto ciò che una corsa **produce** va sulla
status line insieme a progresso e cancel, cioè `.status__transcribe-error` e l'anteprima cue
(`.status__transcribe-cues` o, se ingombra, un elenco che compare solo a corsa finita). Poi
dichiarare esplicitamente, nella riga di riaggancio di T3, che la verifica 3 riapre il dialogo prima
di asserire su `.transcribe__start`, e che la verifica 5 asserisce l'errore sulla status line con la
stessa stringa (`toContain("checksum")`), il che è un riaggancio, non un indebolimento. Se l'owner
risponde alla domanda 2 preferendo che il dialogo resti aperto durante la corsa, questo rilievo
sparisce da solo: annotarlo nella domanda, perché oggi la domanda è presentata come indifferente al
resto del piano ("nothing else in this plan changes"), e non lo è.

---

### B5 — T6 promette di conservare un'asserzione che dopo T6 non ha più modo di essere prodotta

**Task:** T6.

**Difetto:** T6 dichiara "The five existing checks keep every assertion", ma toglie l'unica strada
che può consegnare all'app un percorso di file **inesistente**, che è quello che una di quelle
asserzioni richiede.

**Perché.** `project.spec.js:247-249` scrive `path.join(userFolder, "no-such-file.srt")` dentro
`.project__file-path`, clicca `.project__attach` e pretende `NO_SUCH_FILE` ("There is no file at
that path."). Dopo T6 non esiste più nessun campo dove scrivere un percorso: l'attach passa dal
chooser. Un `GtkFileChooser` in modalità **apertura** non conferma un percorso che non esiste: o
rifiuta o mostra il proprio errore, e l'app non riceve mai la stringa. L'asserzione diventa
non pilotabile, e l'unica uscita è indebolirla o cancellarla, che è §5.4.

**Correzione concreta.** Due parti.

1. Aggiungere alla lista delle cose che il **rapporto di T1 deve rispondere** (oggi elenca titolo,
   inserimento del percorso, conferma, annullamento, tempo di teardown, modalità salvataggio) la
   domanda che T6 dipende da: _un chooser in modalità apertura può confermare un percorso che non
   esiste?_ Se la risposta è no, T6 lo sa prima di iniziare invece di scoprirlo a metà.
2. Dare a T6 la rotta di sostituzione, così non resta una decisione all'implementer: scegliere nel
   chooser un file **reale** in una cartella di scratch, cancellarlo dal filesystem nel test, poi
   cliccare `Attach`. L'errore lo produce il backend al momento dell'attach, non il chooser, e
   `NO_SUCH_FILE` resta la stessa stringa con lo stesso significato: "l'app rifiuta di registrare un
   percorso che non punta a niente".

---

## SERI

### S1 — Le due verifiche nuove di T1 non hanno un posto in `project.spec.js` che soddisfi le loro precondizioni

**Task:** T1.

**Difetto:** gli AC di T1 dicono "Open the app with no project" e "With the project open... `Attach`
adds it to the episode", ma T1 impone anche che "The five existing project checks are untouched:
not re-pointed, **not re-ordered**". Nessuna delle due posizioni possibili funziona.

**Perché.** Le cinque verifiche condividono una sessione e uno stato sequenziale. In testa: creare
un progetto prima della verifica 1 rompe `expect(await textOf(".project__status")).toBe(NO_PROJECT)`
(`project.spec.js:162`) senza toccarne una riga, che è il modo peggiore di romperla. In coda: la
verifica 5 ha appena cancellato il progetto, quindi non c'è progetto aperto né episodio a cui
allegare, e la status line contiene ancora `projectFolder`, quindi nemmeno "no project open" è vero.

**Correzione concreta.** Scrivere in T1 che le due verifiche vanno **in coda** e aprono con
`await browser.reloadSession()` più `attachToApp()`, esattamente come già fa la verifica 3
(`project.spec.js:207`), su una cartella di scratch nuova; e che la seconda verifica crea il proprio
episodio invece di riusare quello della verifica 2. Correggere di conseguenza il testo dei due AC,
che oggi descrivono un'app appena avviata.

---

### S2 — Tutta la catena da T3 in poi è dietro N2, che non è iniziato, e il piano non offre un ordine di ripiego

**Task:** T3, e per trascinamento T5a, T5b, T6, T7, T8.

**Difetto:** `BACKLOG.md` marca N2 `[ ]`. T3 dichiara "Depends on. N2", ma il grafo di §5 non ha un
nodo N2 e presenta `T1 → T2 → T3 → …` come una sequenza eseguibile. Sei task su otto sono dietro un
lavoro non cominciato.

**Perché conta per l'incrementalità.** T4 dichiara "Depends on. Nothing structural. Merges after T3
because both touch `src/App.tsx`". È una dipendenza di comodo, non di sostanza: se N2 slitta, il
piano ferma anche T4, che non ne ha bisogno.

**Correzione concreta.** Mettere N2 nel grafo come predecessore esplicito di T3, e aggiungere una
riga d'ordine alternativo: se N2 non è ancora consegnato quando T2 finisce, si esegue **T4 prima di
T3** (le due ownership sono già dichiarate disgiunte in T2/T4) e T3 rientra appena N2 atterra. Vale
anche la pena scrivere che il nome `video_set_visible` è un'ipotesi e che T3 apre con una lettura
della firma reale, non con una grep di conferma a metà lavoro.

---

### S3 — Non è deciso se il pannello video esiste senza video, e `.stage__empty` è il gate di prontezza di due spec

**Task:** T5a (ma la decisione manca in tutto il piano).

**Difetto:** `shell-layout.md` adotta la regola di Aegisub, "no video open means no video panel at
all", e §2.1 del piano deriva la visibilità da `videoLoaded && videoPanelMounted && layerCount === 0`,
cioè presuppone che il pannello possa essere smontato. Nessun task consegna quella regola, e nessun
AC di T5a la nomina. La mappa dei selettori intanto tiene `.stage__empty` "byte for byte".

**Perché conta.** `.stage__empty` è il segnale di prontezza di due spec:
`video.spec.js:73` e `asr.spec.js:160` aspettano `document.querySelector(".stage__empty") === null`
per dire "il video è pronto". Se il pannello sparisce quando non c'è video, quella condizione è vera
**da subito**, e le due attese passano a vuoto invece di aspettare. È un'asserzione indebolita per
effetto collaterale di una regola di layout, il caso peggiore perché nessuno lo scrive nella
descrizione di consegna. In più il close gate non apre mai un video: la posizione della griglia con e
senza pannello video è diversa, e `FIRST_CUE_TEXT` va misurato nello stato **senza video**, cosa che
T5b non dice.

**Correzione concreta.** Decidere in T5a, per iscritto: o il pannello resta montato e mostra
`.stage__empty` (che è il comportamento di oggi e non tocca niente), oppure smonta e allora T5a deve
sostituire il gate di prontezza delle due spec con un segnale positivo, per esempio
`.controls__button:not([disabled])` più la presenza della finestra figlia di mpv, che è già il
predicato onesto secondo `docs/reports/n2-probe.md`. Aggiungere comunque a T5b la frase "misurato con
nessun video aperto" accanto alla ricomputazione di `FIRST_CUE_TEXT`.

---

### S4 — T4 non dice se un click singolo continua ad aprire l'editor, e tre verifiche esistenti ci contano

**Task:** T4.

**Difetto:** oggi il click singolo sulla cella di testo apre l'editor
(`onClick={() => beginEdit(index)}`, `CueList.tsx:353`), mentre `shell-layout.md` scrive "Enter or a
double-click opens the editor on the active row". T4 introduce il click come gesto di selezione
("Click a row: that row is the cursor and the only selected row") senza dire cosa ne è dell'editor.

**Perché conta.** Tre verifiche di `editor.spec.js` cliccano **una volta** sul testo di una riga e
subito dopo aspettano `.cuelist__editor`: righe 327-331, 511-515, 549-553. Se T4 adotta la regola del
doppio click, tutte e tre falliscono; se T4 lascia il click singolo, l'AC "Click a row: that row is
the cursor and the only selected row" è ambigua, perché quel click apre anche un editor, e l'Escape
che T4 usa per collassare la selezione è lo stesso Escape che annulla l'editor.

**Correzione concreta.** Scriverlo in T4: il click singolo sposta cursore e selezione **e** continua
ad aprire l'editor sulla riga, esattamente come oggi; il doppio click resta equivalente; l'Escape con
un editor aperto appartiene all'editor e solo con l'editor chiuso collassa la selezione. Con questa
scelta le tre verifiche non si toccano affatto. Se invece si adotta il doppio click, T4 deve
elencare le tre verifiche, dire che aggiunge il secondo click, e dichiararlo nella descrizione di
consegna (WORKFLOW §4); e va rivista la rotta di B1, che passa dalla stessa apertura.

---

### S5 — La verifica di regressione su ctrl+z perde il suo campo di testo a T5b e ne perde un secondo a T6

**Task:** T5b, con ricaduta su T6.

**Difetto:** `editor.spec.js:543-577` è la regressione "ctrl+z dentro un campo di testo appartiene al
campo": scrive un percorso in `.subbar__dest`, preme ctrl+z lì dentro, e poi verifica che l'undo
della toolbar tolga **il primo** passo dalla pila. T5b cancella `.subbar__dest` e la §3 lo elenca fra
i "Gone, gesture replaced" senza indicare un sostituto. L'AC di T5b si limita a dire "Ctrl+Z inside a
text field still belongs to the field" senza dire quale campo.

**Perché conta due volte.** Dopo T5b i campi di testo rimasti sono l'editor inline (che è un caso
diverso: `onListKeyDown` esce già quando `editingRef.current` è impostato) e i campi del rail. Se
l'implementer riaggancia su `.project__path`, **T6 lo cancella**, e T6 non possiede
`editor.spec.js`: la verifica muore in un task che non sa di averla toccata. Se riaggancia su
`.project__new-episode` sopravvive, ma è una scelta che il piano lascia aperta, contro il suo stesso
scopo dichiarato ("no design left to do").

**Correzione concreta.** Nominarlo in §3: `.subbar__dest` → `.project__new-episode` per questa
verifica, con la nota che è l'unico campo di testo che sopravvive a T6, e aggiungere a T6 il vincolo
"non rimuovere né rinominare `.project__new-episode`: `editor.spec.js` ci digita dentro". In
alternativa, se il rail cambia forma, dare a T6 la proprietà di `editor.spec.js` e il riaggancio.

---

### S6 — T7 mette `Close` nel menu File e reintroduce la strada di perdita dati che la decisione 9 ha chiuso

**Task:** T7.

**Difetto:** T7 elenca "File (open video, open subtitle, save, save copy as, **close**)" e poi
dichiara "Every menu item does exactly what its toolbar twin does". `Close` non ha un gemello nella
toolbar di T5b, quindi non ha comportamento definito, e non ha nessun AC.

**Perché conta.** Il comando esiste già e prende un booleano:
`subtitle_close(state, discard: bool)` (`src-tauri/src/subtitle/mod.rs:133-139`). L'unico chiamante
frontend lo invoca con `discard: true` (`src/hooks/useSubtitleFile.ts:184`), dentro
`discardAndOpen`, cioè dopo che l'utente ha già scelto di scartare. Un `File > Close` scritto per
analogia con quel chiamante butta via le modifiche non salvate in silenzio: esattamente il difetto
attivo che la decisione 9 ha classificato come perdita dati e messo davanti a ogni milestone
funzionale. Il close gate di N1 intercetta `CloseRequested` sulla finestra e non vede questa strada.

**Correzione concreta.** O togliere `Close` da T7 e rimandarlo al task che porta anche il suo gate
(la §1 di questo piano ha già la regola giusta: "Any menu title with no working item behind it.
Menus arrive with their milestone"), oppure tenerlo e aggiungergli l'AC osservabile: "con modifiche
non salvate, `File > Close` chiede salva / scarta / annulla e onora la risposta; annulla lascia il
documento sullo schermo e il file su disco intatto", più la verifica corrispondente. Non lasciarlo
elencato senza comportamento.

---

## MINORI

### M1 — L'AC anti-clipping di T5a guarda solo lo scroll orizzontale, e solo il puntatore sul video

**Task:** T5a.

**Difetto:** l'AC dice "no control is cut off at a window edge and the page has no **horizontal**
scrollbar", e la verifica del rotellino è "Put the pointer **over the video panel** and scroll".
Manca il caso che romperebbe davvero il vincolo M0.2: un **overflow verticale del documento**.

**Perché conta.** `VideoStage` ricalcola la regione su `ResizeObserver` e su `window resize`
(`VideoStage.tsx:41-43`). Uno scroll verticale del documento cambia
`getBoundingClientRect().top` **senza** cambiare la dimensione dell'elemento: nessuna delle due
callback scatta, e la superficie nativa resta dove era mentre il pannello si muove sotto. È
esattamente il fallimento che la §6 del piano vieta ("The video panel and every ancestor of it never
scroll"), e nessun AC lo misura. Il rischio non è teorico a 1024x700: T5a somma la striscia
parcheggiata, la banda alta, la griglia e la nuova status line, e gli stati transitori aggiungono
altezza (riga d'errore video, riga d'errore documento, bottone Discard che compare, progresso
trascrizione sulla status line dopo T3).

**Correzione concreta.** Aggiungere all'AC di T5a, a entrambe le dimensioni: `document.scrollingElement.scrollHeight === document.scrollingElement.clientHeight`
(nessuno scroll verticale del documento), e allargare la verifica del rotellino a un punto **fuori**
dalla griglia e dal rail (per esempio sulla toolbar o sulla status line), confrontando la geometria
X11 della superficie prima e dopo. Farlo nello stato più alto possibile: progetto aperto, video
aperto, file aperto, riga d'errore visibile, bottone Discard presente.

### M2 — T5b sostituisce due coordinate fisse con una terza che il piano non nomina

**Task:** T5b.

**Difetto:** "SUBTITLE_PATH_FIELD and SUBTITLE_OPEN_BUTTON are replaced by a click on the toolbar's
open-subtitle control plus the T1 helper". Quel click, in uno script senza DOM, è a sua volta una
coordinata assoluta, che il piano non elenca e non dice come derivare. Il conteggio "una attesa fissa
in meno" resta vero, ma il debito di fragilità non si riduce come suggerito.

**Correzione concreta.** Nominare la costante e dire dove si misura (toolbar, riga 1, a 1024x700,
senza video aperto), e ripetere l'AC "12/12" nella consegna.

### M3 — "Le tre barre parcheggiate" sono due dopo T3

**Task:** T5a e T5b.

**Difetto:** T5a dice "The three bars are parked" e T5b "The three parked workspace bars are
deleted", ma T3 ha già trasformato la banda di trascrizione in un dialogo e cancellato
`TranscribeBar.tsx`. Ne restano due: `VideoOpenBar` e `SubtitleBar`. Il conteggio è residuo di §0,
dove le bande sono cinque.

**Correzione concreta.** Scrivere "le due barre rimaste (`VideoOpenBar`, `SubtitleBar`)" in entrambi
i punti. È una riga, ma un implementer che cerca la terza barra perde tempo o inventa.

### M4 — `budget-check.js` (T8) eredita la stessa fragilità del close gate, moltiplicata

**Task:** T8.

**Difetto:** `budget-check.js` "spawns the app, opens the 2,000-cue fixture and the video fixture
through the chooser" ed è, come il close gate, uno script senza sessione WebDriver. Quindi due
aperture pilotate a coordinate assolute su toolbar e chooser, in un file nuovo, e nessuna riga del
piano lo dice.

**Correzione concreta.** Dire in T8 che `budget-check.js` riusa l'helper di T1 e le costanti di
coordinate del close gate estratte in `e2e/lib/`, invece di duplicarle: una sola definizione della
geometria della toolbar, così il prossimo cambio di guscio ha un solo posto da aggiornare.

---

## Cosa il piano fa bene, per contrasto

Non tutto è da rifare, e vale la pena dirlo perché tre scelte sono esattamente quelle giuste per
l'incrementalità.

- **T1 primo e da solo**, senza una riga di codice di produzione, con la clausola "If it does not
  work, stop". È il modo corretto di ordinare un rischio esterno.
- **Le barre parcheggiate di T5a.** Un intermedio dichiaratamente brutto in cambio di due consegne
  revisionabili invece di una illeggibile: è la scelta giusta e il piano la motiva.
- **T3 prima di T5a**, cioè il pezzo più rischioso (registro dei layer e occlusione) provato dentro
  il layout vecchio mentre nient'altro si muove. Difendibile e ben argomentato.

Il difetto sistematico non è l'ordine dei task: è che il piano modella l'incrementalità **solo sulla
suite mocha** (i contatori `EXPECTED_TESTS` da 27 a 45 sono tracciati task per task) e tratta
`close-gate-check.js` come un file da toccare una volta sola, quando invece è la verifica più
sensibile al guscio dell'intera batteria e gira in CI su ogni push. Quattro dei sei rilievi gravi qui
sopra escono da quella singola omissione.

---

## Nota di onestà (CLAUDE.md §9)

Niente qui è stato eseguito. L'app non è stata avviata, la batteria non è stata lanciata, nessuna
geometria è stata misurata a schermo. B1 in particolare poggia su un'aritmetica ricavata dal CSS
(`flex: 1 1 45%` / `flex: 1 1 55%`, banda ASR `flex: none`, `ROW_HEIGHT = 28`, fixture da 3 cue) e
non da una misura: la **direzione** dello spostamento è certa, l'entità è stimata in circa 40 px, e
qual è la riga colpita oggi non lo so. La correzione proposta vale comunque, perché il problema
reale è che nessun task ha il compito di misurare.

Tutto il resto (contraddizioni interne al piano, asserzioni esistenti e loro selettori, firme dei
comandi, ownership dei file, presenza del close gate in CI) è verificato leggendo i file citati, con
riga o intervallo di righe accanto a ogni affermazione.
