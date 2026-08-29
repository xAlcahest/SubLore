# Post-v1 plan — drafted 2026-08-29

Produced by a 23-agent scan of Aegisub's source (251 commands, 8 domains, every command accounted for), each feature then weighed against Sublore's own code, three adversarial passes, and a synthesis. Aegisub reference: `arch1t3cht/Aegisub` and `TypesettingTools/Aegisub`, BSD-3-Clause, read for structure only.

Everything here comes **after** v1.0 (M2.4, M2.5, M2.6, M5, M6). Nothing in this file changes the current plan except the section "Decisions due now", which does.

> **Superseded 2026-08-29:** all thirteen questions in "Decisions due now" have been ruled on by the owner. The rulings and their reasoning are in `decisions.md`, and the work is in `../../BACKLOG.md`. The section below is kept as the record of how each question was framed, not as an open question. Decision 8 lands in M11 and decision 11 (milliseconds, final) confirms the frame exclusions already listed under "What we will not do".

## Summary

Piano post-v1 in dieci milestone, ordinate per valore al traduttore diviso costo. Ho riverificato di persona ogni fatto portante, perche la passata avversariale aveva trovato citazioni fabbricate: le conclusioni del materiale reggono quasi tutte, l'evidenza a volte no. Tre scoperte cambiano l'ordine rispetto a quello che l'inventario suggeriva. Primo: cinque comandi di mutazione sono registrati (src-tauri/src/lib.rs:45-49) e il frontend non ne chiama nessuno (src/hooks/useSubtitleFile.ts:262-281 espone solo setText, undo, redo, save, saveAs), quindi oggi non si puo cancellare una riga, inserirne una, dividerla o unirla. Secondo: la superficie video nativa si rialza sopra il webview a ogni aggiornamento di regione (src-tauri/src/video/surface/mod.rs:80, surface/linux.rs:66, surface/windows.rs:2), quindi occlude i menu a tendina e i dialoghi HTML che M2.0 mette proprio li (docs/design/shell-layout.md:68,105), e non esiste alcun percorso che la rimostri dopo averla nascosta (src-tauri/src/video/mod.rs:196-197, show solo dentro video_open a :106). Terzo: il livello di test primario gira solo su X11 (.github/workflows/ci.yml:125-126, e2e/lib/input.js:6-9) mentre M0.2 e ancora aperto con Windows non verificato (BACKLOG.md:16-18), quindi ogni "gia presente" del dominio video vale su meta della matrice di rilascio. Le prime tre voci del piano non sono funzioni appariscenti: sono le operazioni di riga, la ricerca e la riparazione dei tempi, cioe le tre cose la cui assenza impedisce di lavorare. Il typesetting e diviso in due milestone lontane fra loro, perche il proprietario deve poterle decidere separatamente: gli stili come dati (M14) non toccano il video, il typesetting visuale (M16) impone di sostituire il percorso video e riscrive sei componenti.

## Milestones

| id  | cost   | title                                                | depends on                                                |
| --- | ------ | ---------------------------------------------------- | --------------------------------------------------------- |
| M7  | grande | Selezione multipla e modifiche in blocco             | v1.0 completa (M2.6 e M6)                                 |
| M8  | medio  | Trova, sostituisci, vai a                            | M7                                                        |
| M9  | medio  | Riparazione dei tempi                                | M7                                                        |
| M10 | medio  | I sottotitoli sul video                              | M2.6                                                      |
| M11 | medio  | Non perdere lavoro                                   | M7                                                        |
| M12 | medio  | Qualita del testo consegnato                         | M7                                                        |
| M13 | medio  | File che oggi non si aprono                          | M7                                                        |
| M14 | grande | Gli stili come dati (primo tempo del typesetting)    | M7                                                        |
| M15 | grande | Timing consapevole della scena                       | M2.4, M9                                                  |
| M16 | enorme | Typesetting visuale: sostituzione del percorso video | M14 per i tag, e una decisione esplicita del proprietario |

### M7 — Selezione multipla e modifiche in blocco

**Cost:** grande · **Depends on:** v1.0 completa (M2.6 e M6)

Rendere raggiungibili le mutazioni gia costruite e dare all'editor una selezione vera, con un solo passo di annulla per operazione. E il pezzo che sblocca cinque funzioni piu avanti in questo piano: senza, ognuna reinventa la propria mezza soluzione.

**Contents**

- Selezione multipla nella cue list: insieme di righe piu linea attiva distinta, ctrl-click, shift-click, ctrl+A, stessi modificatori da tastiera. Oggi la selezione e un solo indice React (src/components/CueList.tsx:85), usato sia dal click sia dall'editor.
- Modifica in blocco in sublore-edit: oggi ogni voce di cronologia porta una sola Splice (crates/sublore-edit/src/history.rs:37-39) e l'enum Edit nomina una cue sola (crates/sublore-edit/src/plan.rs:28-58). La fusione delle voci non aiuta, perche richiede stessa etichetta e stesso offset (crates/sublore-edit/src/history.rs:192-195), quindi due modifiche su cue diverse non si fondono mai. Serve un'operazione che emetta una splice unica sul range toccato, con l'Expectation estesa a coprirla.
- Cablare al frontend i comandi registrati e mai chiamati: subtitle_insert, subtitle_delete, subtitle_split, subtitle_merge, subtitle_set_times (src-tauri/src/lib.rs:45-49). M2.5 raggiunge solo i tempi, e solo trascinando sulla waveform: gli altri quattro restano irraggiungibili anche a v1 finita.
- Operazioni di riga dalla griglia con scorciatoia: elimina, inserisci sopra, inserisci sotto, duplica, dividi al cursore, unisci con la successiva.
- Il gate delle modifiche non salvate e la guardia di revisione esistente (src-tauri/src/subtitle/mod.rs) vanno estesi al caso multiplo, non aggirati.

**Acceptance criteria** (observable behaviour, per CLAUDE.md §5)

- Apri una fixture da fixtures/subtitles/srt/, seleziona le righe 3, 4, 5, 6 e 7 con shift-click e premi Canc: le cinque righe spariscono, il conteggio scende di cinque, un solo Ctrl+Z le riporta tutte e salvando subito dopo il file e byte-identico a quello di partenza.
- Seleziona le righe 3 e 12 con ctrl-click e premi Canc: spariscono solo quelle due, la riga 7 resta dov'era col suo testo intatto, e un solo Ctrl+Z le riporta entrambe.
- Con due righe adiacenti selezionate, il comando unisci lascia una riga sola il cui testo e la concatenazione col separatore di riga del formato e i cui tempi vanno dall'inizio della prima alla fine della seconda; un solo Ctrl+Z rispalca entrambe.
- Con una riga sola selezionata ogni comando si comporta come prima: i controlli E2E esistenti sull'editing di una cue passano con le asserzioni invariate e nessuno skip.
- Sulla fixture da 2000 cue, Ctrl+A seguito da freccia giu non blocca l'interfaccia oltre il budget di CLAUDE.md sezione 7, misurato e riportato.
- Su una fixture ASS, eliminare una selezione che attraversa righe Comment: lascia le sezioni non interpretate identiche byte per byte.

### M8 — Trova, sostituisci, vai a

**Cost:** medio · **Depends on:** M7

Dare al traduttore il gesto che oggi non ha in nessuna forma: ritrovare una battuta in ottocento righe e propagare una resa cambiata. E anche il motore di confronto testuale che il QA della serie consuma, quindi va costruito una volta e nel core aperto.

**Contents**

- Ricerca su testo, e sui campi evento ASS dove ha senso, con maiuscole significative o no, espressione regolare, e limite alla selezione. Oggi non esiste nulla: nessuna dipendenza regex nel workspace, nessuna stringa di ricerca in src/i18n/en.ts.
- Scanner dei tag override ASS con rimappatura degli offset: cercare nel testo visibile come se i blocchi graffa non ci fossero, e riscrivere senza toccarli. Oggi il parser conserva il testo come span grezzo e dichiara di non leggerlo (crates/sublore-formats/src/ass.rs:1-7).
- Il motore vive in un crate aperto e condiviso, non dentro il modulo chiuso M5. CLAUDE.md sezione 4 vuole il core aperto pienamente utile da solo, e il QA termbase ha bisogno esattamente dello stesso confronto.
- Sostituisci tutto come una sola operazione annullabile, sopra la macchina di M7.
- Vai alla riga per numero e per tempo.

**Acceptance criteria** (observable behaviour, per CLAUDE.md §5)

- Apri una fixture da fixtures/subtitles/ass/ contenente tag override, cerca una parola che compare sia nel testo visibile sia dentro un tag: con salta-i-tag attivo la griglia evidenzia solo le occorrenze nel testo, e sostituendole i blocchi graffa restano identici byte per byte.
- Su una fixture dove il termine compare in dodici righe, sostituisci tutto: le dodici righe cambiano, tutte le altre restano byte-identiche, il conteggio riportato all'utente e dodici, e un solo Ctrl+Z riporta tutte e dodici.
- Con limite alla selezione attivo e le righe 10-20 selezionate, un termine presente anche alla riga 5 non viene toccato e il conteggio non lo include.
- Una ricerca senza risultati lo dice esplicitamente e non sposta la selezione.
- Trova successivo dalla fine del file ricomincia dall'inizio e si ferma dopo aver fatto un giro completo, senza ciclare all'infinito.

### M9 — Riparazione dei tempi

**Cost:** medio · **Depends on:** M7

Rendere lavorabile un file che arriva sfasato e rifinire in un colpo i tempi grezzi che escono dalla trascrizione. Un sorgente disallineato di due secondi oggi non e riparabile in alcun modo dall'interfaccia.

**Contents**

- Spostamento globale dei tempi in millisecondi, con ambito tutte le righe, solo la selezione, o dalla riga corrente in avanti, avanti o indietro, su inizio e fine o solo su uno dei due. Il ramo tutte-le-righe non richiede selezione e va fatto per primo.
- Post-processore dei tempi in una passata: lead-in e lead-out con controllo delle collisioni, e continuita fra righe adiacenti entro soglie di distanza e sovrapposizione, con un bias che decide chi cede tempo. Nessuna dipendenza dal video o dai keyframe.
- Lead-in e lead-out applicati alla selezione, oltre alla riga singola che M2.5 gia copre.
- Rendi continui: inizio di ogni riga alla fine della precedente, e il gemello sulla fine. Utile dopo un ritaglio.
- Tutto passa dalla macchina di blocco di M7, quindi ogni operazione e un passo di annulla solo.

**Acceptance criteria** (observable behaviour, per CLAUDE.md §5)

- Apri una fixture SRT e applica uno spostamento globale di meno 2000 millisecondi: ogni cue arretra esattamente di 2000, la prima cue che finirebbe prima di zero viene gestita secondo la regola dichiarata invece che silenziosamente troncata, e un solo Ctrl+Z riporta tutto.
- Su una fixture ASS, uno spostamento che produrrebbe un millisecondo non rappresentabile nella forma del file viene rifiutato con un messaggio leggibile e nulla viene scritto, invece di essere arrotondato in silenzio.
- Prendi un episodio trascritto (fixtures/asr/) ed esegui il post-processore con lead-in 200 e lead-out 300: le righe che avrebbero spazio si allargano, le due righe che collidono con la vicina restano ai tempi originali, nessuna coppia risulta sovrapposta, e il tutto e un solo passo di annulla.
- Con le righe 4-9 selezionate, rendi continui: la fine di ognuna coincide con l'inizio della successiva dentro quel blocco, la riga 3 e la riga 10 non cambiano di un millisecondo.
- Salvando dopo ognuna di queste operazioni, le sezioni e i campi che il comando non tocca restano byte-identici.

### M10 — I sottotitoli sul video

**Cost:** medio · **Depends on:** M2.6

Vedere la riga che si sta traducendo sul fotogramma, mentre la si scrive. Oggi l'app non mostra sottotitoli sul video in nessuna forma, e questo non richiede di toccare l'architettura video: mpv rende con la propria libass dentro la propria superficie.

**Contents**

- Caricare il documento in mpv. Oggi e disattivato per configurazione: sub-auto vale no (src-tauri/src/video/player.rs:41) e nessun punto del codice chiama sub-add.
- Copia ombra del buffer in corso di modifica, scritta nella cartella di lavoro di Sublore con una frequenza limitata, mai attraverso il percorso di salvataggio. Passare dal salvataggio significherebbe sovrascrivere il file dell'utente e ruotare l'anello dei backup da dieci (crates/sublore-io/src/backup.rs:21, crates/sublore-io/src/atomic.rs:59-74) a ogni tasto premuto.
- Ciclo di vita della traccia in mpv: aggiunta all'apertura, ricarica alla modifica, rimozione alla chiusura del documento, e gestione del caso in cui il video venga chiuso per primo.
- Con M2.6 in piedi, scelta esplicita di quale dei due documenti si vede sul video.

**Acceptance criteria** (observable behaviour, per CLAUDE.md §5)

- Apri un video e il suo sottotitolo, metti in pausa su un tempo dove una battuta e attiva: quella battuta si vede sul fotogramma.
- Modifica il testo di quella riga e non salvare: entro un secondo il video mostra il testo nuovo, e il file del sottotitolo su disco ha dimensione e ora di modifica invariate.
- Annulla la modifica: il video torna a mostrare il testo di prima.
- Chiudi il sottotitolo tenendo aperto il video: il video continua a riprodursi e non mostra piu alcun testo.
- Dopo una sessione di venti minuti di editing, la cartella dei backup dell'utente contiene esattamente le copie prodotte dai salvataggi espliciti, nessuna in piu, e la copia ombra e sparita alla chiusura dell'app.
- Con sorgente e traduzione aperti, cambiare quale dei due si vede cambia il testo sul fotogramma e non tocca nessuno dei due file.

### M11 — Non perdere lavoro

**Cost:** medio · **Depends on:** M7

Chiudere l'ultimo percorso per cui una sessione di lavoro puo evaporare, e far ripartire un episodio da dove lo si era lasciato. Il salvataggio atomico e i backup pre-sovrascrittura esistono e sono collaudati (M1.4); quello che manca e la rete sotto il buffer non ancora salvato.

**Contents**

- Autosave periodico delle sessioni modificate, con il proprio spazio di archiviazione e il proprio tetto, separati da quelli dei backup pre-sovrascrittura. Condividere BackupStore sfratterebbe le copie di sicurezza dell'utente in dieci scatti (crates/sublore-io/src/backup.rs:21, prune a :208-216).
- Nessuna riscrittura quando nulla e cambiato: la sessione sa gia dire se e sporca.
- Sfoglia recuperi: elenco delle copie di emergenza per file, con orario, e apertura di quella scelta. Cancellare una copia resta un gesto dell'utente, mai una pulizia automatica (CLAUDE.md sezione 3.3).
- Ripresa dell'episodio: posizione di riproduzione, riga attiva e posizione di scorrimento salvate nel database di progetto, che ha gia il suo runner di migrazioni versionato. Nel database, mai dentro il file di sottotitoli dell'utente.

**Acceptance criteria** (observable behaviour, per CLAUDE.md §5)

- Modifica cinque righe senza salvare, uccidi il processo, riavvia l'app: l'elenco dei recuperi mostra una copia con l'orario giusto, e aprendola le cinque modifiche ci sono tutte.
- Salva lo stesso file dieci volte di seguito con l'autosave attivo: alla fine ci sono dieci backup pre-sovrascrittura, uno per salvataggio, e nessuno e stato sfrattato dalle copie di autosave prodotte nel frattempo.
- Lascia l'app ferma per mezz'ora senza toccare nulla: non compare alcuna copia di autosave nuova.
- Apri un episodio di un progetto, portati a meta video su una riga qualsiasi, chiudi l'app, riaprila e riapri lo stesso episodio: video e sottotitolo si riaprono, la riga attiva e la stessa, la posizione di riproduzione e la stessa entro un secondo.
- Il file di sottotitoli dell'utente non contiene nessuna sezione nuova dopo tutto questo: il round-trip resta byte-identico.

### M12 — Qualita del testo consegnato

**Cost:** medio · **Depends on:** M7

I controlli che riguardano il testo che il traduttore consegna, non il timing. Il primo gradino e quasi gratis e oggi e spento a mano.

**Contents**

- Riaccendere il correttore del webview nell'editor di riga. Oggi e disattivato esplicitamente (src/components/CueList.tsx:345, spellCheck impostato a false): zero dipendenze, zero rete, zero policy da rinegoziare.
- Secondo gradino, opzionale e da decidere col proprietario: motore ortografico lato Rust con scelta della lingua e tokenizzazione che salta i tag override. Richiede una dipendenza nuova compatibile GPL e, soprattutto, una decisione sui dizionari, perche CLAUDE.md sezione 1 elenca le uniche eccezioni di rete ammesse e i dizionari non sono fra quelle.
- Colonna caratteri al secondo con soglie colorate. La colonna e gia nel disegno della griglia M2.0 (docs/design/shell-layout.md:77) e il valore e calcolabile nel frontend dai campi gia in mano, quindi il lavoro qui e la soglia, il colore e il filtro.
- Filtro nella griglia: mostra solo le righe che superano la soglia, per fare una passata di rilettura.

**Acceptance criteria** (observable behaviour, per CLAUDE.md §5)

- Apri l'editor su una riga contenente una parola inesistente nella lingua del target: la parola e sottolineata e il menu contestuale del sistema propone alternative.
- Con il motore lato Rust attivo, una parola dentro un blocco graffa di un ASS non viene segnalata come errore, mentre la stessa parola nel testo visibile si.
- Apri una fixture con una riga a 25 caratteri al secondo e una a 12, con soglia a 20: la prima e marcata nella griglia, la seconda no.
- Accorcia il testo della riga marcata mentre la si edita: la marcatura sparisce senza dover salvare ne riaprire.
- Attiva il filtro per soglia: la griglia mostra solo le righe sopra soglia, il conteggio lo dice, e togliendo il filtro l'elenco torna completo con la stessa riga attiva.

### M13 — File che oggi non si aprono

**Cost:** medio · **Depends on:** M7

Allargare cio che l'app accetta in ingresso e cio che sa produrre in uscita. Oggi un file non UTF-8 non si apre affatto, per scelta esplicita: chi lavora su archivi fansub storici trova la porta chiusa.

**Contents**

- Rilevamento della codifica in lettura e scelta manuale quando il rilevamento e incerto. Oggi il lettore rifiuta BOM UTF-16 e UTF-32, byte NUL nei primi kilobyte e UTF-8 non valido, senza mai transcodificare (crates/sublore-formats/src/text.rs:41-42, il commento dice che un file che potremmo decodificare male e un file che rifiutiamo).
- Decisione di policy prima del codice: cosa promette il round-trip senza perdite per un file transcodificato. Le tre opzioni oneste sono riscrivere nella codifica d'origine, convertire a UTF-8 dichiarandolo all'utente, o continuare a rifiutare. Il codice attuale riemette gli stessi byte del sorgente, quindi la terza opzione non costa nulla e le altre due richiedono un serializzatore che oggi non esiste.
- Import della traccia sottotitoli da un contenitore MKV, estraendola con ffmpeg che e gia strumento obbligatorio scoperto all'avvio (crates/sublore-asr/src/tools.rs:36-45) verso la cartella di lavoro, senza mai toccare il file video (CLAUDE.md sezione 3.1). Serve aggiungere l'enumerazione delle tracce, oggi assente perche ffprobe non e nella scoperta.
- Conversione vera fra i tre formati supportati. Oggi salva-con-nome riemette gli stessi byte nello stesso formato (src-tauri/src/subtitle/mod.rs:390-391 e commento a :390), quindi salvare un SRT come .ass produce un SRT rinominato. Serve il concetto di degradazione dichiarata: ricombinazione delle sovrapposizioni, rimozione dei tag, conversione dei ritorni a capo, con l'avviso di cosa si perde.

**Acceptance criteria** (observable behaviour, per CLAUDE.md §5)

- Apri un SRT codificato in CP1251 con testo cirillico: il file si apre, il testo e leggibile nella griglia, la codifica rilevata e mostrata, e la policy scelta dal proprietario e visibile all'utente prima che salvi.
- Apri un file con BOM UTF-16: viene rilevato, non produce piu un errore di grammatica quaranta righe dentro, e il messaggio dice che codifica e.
- Apri un MKV con due tracce sottotitoli: l'app elenca le due tracce con la loro lingua, estrae quella scelta e la apre nella griglia; il file MKV ha dimensione e ora di modifica invariate.
- Esporta in SRT un ASS con due righe sovrapposte e tag override: il risultato non ha sovrapposizioni, non ha tag, i ritorni a capo sono quelli di SRT, e l'ASS di partenza e byte-identico a prima.
- Prima di esportare, l'app dice esattamente cosa il formato di destinazione non regge; annullando, nulla viene scritto.

### M14 — Gli stili come dati (primo tempo del typesetting)

**Cost:** grande · **Depends on:** M7

Modellare e modificare gli stili ASS senza toccare nulla del video. E la meta del typesetting che il proprietario puo decidere da sola, e la fondazione di tutto il resto: senza modello degli stili non esistono editor di stile, assistente stili, ne ricampionamento. Sono onesto sul valore per il traduttore: e basso. Il valore vero e l'adozione di chi arriva da script ASS gia impaginati, piu la fondazione.

**Contents**

- Modello dati degli stili: leggere le righe Style: che oggi viaggiano come metadati non interpretati (crates/sublore-formats/src/document.rs:81-92 SegmentKind::Meta, crates/sublore-formats/src/ass.rs:1-7).
- Una mutazione che sappia scrivere segmenti non-cue. Oggi l'enum Edit copre solo le cue (crates/sublore-edit/src/plan.rs:28-58) e la verifica costruisce le sue attese sulle cue: e il collo di bottiglia comune a stili, proprieta dello script e allegati.
- Editor di uno stile, con propagazione della rinomina a tutti gli eventi che lo usano. I campi evento ASS sono gia conservati come span in ordine di Format (crates/sublore-formats/src/cue.rs:49-58): va nominato il campo Style.
- Gestore stili e catalogo a livello di serie nel database di progetto, che ha gia il runner di migrazioni versionato. Non in un file per utente come in Aegisub: la serie e la nostra unita naturale.
- Proprieta dello script e gestione degli allegati, stesso collo di bottiglia. La conservazione degli allegati e gia garantita e coperta da test, quindi qui si aggiunge solo la gestione.
- Anteprima di uno stile su testo campione, se il proprietario la vuole: si disegna in un riquadro proprio, non sopra il fotogramma, quindi non tocca il percorso video. Costa una dipendenza di rasterizzazione nuova.

**Acceptance criteria** (observable behaviour, per CLAUDE.md §5)

- Apri un ASS con tre stili e rinomina il secondo: ogni evento che lo usava porta il nome nuovo, gli eventi che usavano gli altri due sono byte-identici, le sezioni non interpretate sono byte-identiche, e un solo Ctrl+Z annulla tutto.
- Cambia la dimensione del carattere di uno stile e salva: nel file cambia solo quel campo di quella riga Style:, e il resto del file e byte per byte quello di prima.
- Salva gli stili di un episodio come catalogo della serie, apri un secondo episodio della stessa serie e applica il catalogo: gli stili compaiono nel file del secondo episodio, gli eventi non cambiano, e riaprendo il progetto il catalogo e ancora li.
- Modifica il titolo dello script e PlayResX nelle proprieta: cambiano solo quelle due chiavi, le altre righe di Script Info sono identiche.
- Estrai un font allegato su disco e riaprilo: il file estratto e byte-identico a quello che l'ASS conteneva, e l'ASS non e cambiato.

### M15 — Timing consapevole della scena

**Cost:** grande · **Depends on:** M2.4, M9

Far coincidere inizio e fine delle battute con i cambi di inquadratura. E lavoro da timer piu che da traduttore, e lo dico apertamente: sta qui e non prima perche il traduttore che riceve un sorgente gia timmato non lo usa mai. Serve invece a chi rifinisce l'output di whisper su animazione.

**Contents**

- Estrazione dei keyframe dal media. Oggi non esiste alcuna nozione di keyframe, fotogramma o framerate in tutto il codice di produzione, e libmpv non espone la lista. Serve estenderla scoperta degli strumenti (oggi solo ffmpeg, crates/sublore-asr/src/tools.rs:36) e una cache per episodio nel database di progetto.
- Marcatori di scena disegnati sulla waveform di M2.4, che e canvas HTML in un pannello a fianco del video e quindi non tocca la superficie nativa.
- Aggancio alla scena: inizio e fine della cue portati ai confini dell'inquadratura che contiene il playhead.
- Sezione di aggancio ai keyframe nel post-processore di M9, disattivabile quando i keyframe non ci sono.
- Import ed export di liste keyframe da file esterno solo se il proprietario lo chiede: e vocabolario da fansub avanzato e il costo incrementale e piccolo, ma da solo non produce valore.

**Acceptance criteria** (observable behaviour, per CLAUDE.md §5)

- Apri un video di prova con tagli a tempi noti: la waveform mostra i marcatori di scena a quei tempi, entro la tolleranza dichiarata.
- Con una cue selezionata e il playhead dentro un'inquadratura, esegui aggancia alla scena: inizio e fine della cue coincidono con i confini di quella inquadratura, le cue vicine non cambiano, e un solo Ctrl+Z riporta i tempi di prima.
- Con il playhead prima del primo cambio scena, il comando non fallisce e usa l'inizio del file come confine.
- Riapri lo stesso episodio: i keyframe non vengono riestratti, e i marcatori compaiono subito.
- Su un media senza keyframe estraibili, la sezione di aggancio del post-processore e disattivata e spiega perche, invece di produrre risultati sbagliati.

### M16 — Typesetting visuale: sostituzione del percorso video

**Cost:** enorme · **Depends on:** M14 per i tag, e una decisione esplicita del proprietario

Poter disegnare e ricevere il mouse sul fotogramma, che e il prerequisito di ogni strumento visuale: mirino con coordinate, trascinamento della posizione, rotazione, scala, ritaglio, prospettiva, maschera overscan. Questa e la milestone piu cara del piano e la sola davvero bloccata dall'architettura. Va decisa come una milestone a se, mai infilata dentro un'altra.

**Contents**

- Sostituire il percorso wid con la render API di libmpv. Oggi mpv riceve la maniglia di una finestra figlia nativa (src-tauri/src/video/player.rs:186-188) creata al setup della finestra principale.
- Cosa si riscrive, per intero e non in parte: il modulo superficie e i suoi due backend di piattaforma (src-tauri/src/video/surface/mod.rs, linux.rs, windows.rs), il comando video_set_region e il posizionamento da DOM (src/components/VideoStage.tsx), la coreografia di spegnimento che esiste proprio perche mpv disegna dentro una finestra figlia (src-tauri/src/lib.rs, gestione di CloseRequested), e i controlli E2E che ispezionano la finestra nativa con xwininfo (e2e/lib/x11.js).
- Cosa smette di essere un problema: l'occlusione dei menu e dei dialoghi, l'assenza di un percorso di ri-mostra, l'opacita all'input (src-tauri/src/video/surface/linux.rs:44 imposta pass-through a falso, e src-tauri/src/video/player.rs:44-46 spegne l'input lato mpv), e il vincolo per cui il pannello del video non puo mai scorrere.
- Cosa si rischia: i budget di CLAUDE.md sezione 7 (avvio sotto due secondi, memoria a riposo sotto 400 MB). Un giro di texture per fotogramma dentro un webview e il modo classico di sfondarli, e il risultato va misurato prima di considerare fatta la milestone, non dopo.
- Gli strumenti visuali veri e propri (mirino, trascinamento, rotazione, scala, ritaglio, prospettiva) sono contenuto successivo e presuppongono anche il modello semantico dei tag override, che M14 non fornisce: M14 da gli stili, non i tag.

**Acceptance criteria** (observable behaviour, per CLAUDE.md §5)

- Il video si vede dentro la finestra come prima, e i controlli E2E esistenti passano con le asserzioni invariate, nessuna riscritta al ribasso e nessuna saltata (CLAUDE.md sezione 5.4).
- Apri un menu a tendina che si sovrappone al rettangolo del video: e visibile per intero, e cliccando una sua voce l'azione parte.
- Apri un dialogo modale centrato sopra il video: e visibile per intero e riceve la tastiera.
- Nascondi il pannello video e rimostralo: il video riappare e continua a riprodursi dal punto giusto.
- Clicca sul fotogramma: la riproduzione si mette in pausa. Muovi il mouse sul fotogramma: le coordinate nello spazio dello script sono mostrate e corrispondono, verificate su due punti noti.
- Avvio a freddo e memoria a riposo sono misurati su una macchina di riferimento e riportati nella descrizione della PR, con il confronto rispetto ai numeri di prima della sostituzione.
- Gli stessi controlli passano su Windows e su Linux, entrambi in CI (CLAUDE.md sezione 5.5).

## Decisions due now

These are choices to make **during** the current plan, not after it. Each one, left implicit, gets more expensive to reverse as M2.4-M2.6 and M5 build on top of it.

### 1. Come si comportano menu a tendina e dialoghi sopra il rettangolo del video. La superficie nativa si rialza sopra il webview a ogni aggiornamento di regione (src-tauri/src/video/surface/mod.rs:80 dice testualmente move, resize and raise above the webview; surface/linux.rs:66 e :72 chiamano raise; surface/windows.rs:2 e :62-71 riasseriscono HWND_TOP), e M2.0 mette li sopra una barra di menu HTML (docs/design/shell-layout.md:68) e un dialogo di trascrizione (docs/design/shell-layout.md:105). Le tre strade sono: menu nativi di sistema, oppure nascondere o rimpicciolire la superficie mentre un livello sovrapposto e aperto, oppure popup in finestre separate.

- **Cost today:** Una scelta di design dentro M2.0 e la sua verifica su entrambe le piattaforme. Se si sceglie il menu nativo, e sostituire un componente che deve ancora essere scritto.
- **Cost if deferred:** Ogni menu e ogni dialogo aggiunti in M2.4, M2.5, M2.6, M5 e M6 nascono su un presupposto falso e vanno rifatti tutti insieme. Il proprietario lo scopre aprendo il primo menu sopra un video e vedendolo sparire, cioe alla prima verifica manuale dopo M2.0.

### 2. Se la superficie video sa tornare visibile dopo essere stata nascosta. Oggi apply_region nasconde quando la regione e vuota e altrimenti sposta (src-tauri/src/video/mod.rs:196-197), e show viene chiamato in un solo punto, dentro video_open (src-tauri/src/video/mod.rs:106). Il commento di show avverte che deve avvenire prima che mpv costruisca la propria uscita video, perche mpv crea la sua finestra dentro questa e la lascia non mappata se questa lo e (src-tauri/src/video/surface/mod.rs:82-84). Nascondi e rimostra dopo che il video e gia aperto non e mai stato provato.

- **Cost today:** Un percorso di ri-mostra, un test comportamentale nascondi-poi-rimostra con video gia caricato, e la verifica che mpv rimappi la propria uscita. Mezza giornata piu la sorpresa se mpv non collabora.
- **Cost if deferred:** Il criterio di M2.0 per cui i pannelli si nascondono quando manca il loro provider (docs/design/shell-layout.md:35) funziona in un verso solo e nessuno se ne accorge finche un utente non riapre il pannello. E la soluzione dell'occlusione basata sul nascondere diventa inapplicabile proprio quando serve.

### 3. Quando si verifica la superficie video su Windows e quando l'E2E impara a girare li. Il job check gira su ubuntu e windows (.github/workflows/ci.yml:18), ma l'E2E gira solo su ubuntu (:125-126) e pilota l'app con xdotool via XTEST e ispeziona la finestra con xwininfo (e2e/lib/input.js:6-9), strumenti che su Windows non esistono. M0.2 e ancora aperto e il suo stato dice che Windows e CI restano non verificati (BACKLOG.md:16-18), mentre CLAUDE.md sezione 5.5 fa della matrice verde un requisito di rilascio.

- **Cost today:** Una passata su una macchina Windows piu un backend di input e di ispezione finestra per l'E2E. E lavoro noioso e finito.
- **Cost if deferred:** Tutti i criteri video accumulati da M2.4 a M6 vanno verificati in blocco alla vigilia del rilascio, su un percorso di codice mai eseguito, senza tempo per rimediare. Se il z-order della superficie su Windows si comporta diversamente da quanto il commento assume, si scopre allora.

### 4. La forma della modifica in blocco con un solo passo di annulla, dentro sublore-edit. Oggi ogni voce di cronologia porta una sola Splice (crates/sublore-edit/src/history.rs:37-39), l'enum Edit nomina una cue sola (crates/sublore-edit/src/plan.rs:28-58) e la fusione delle voci richiede stessa etichetta e stesso offset (crates/sublore-edit/src/history.rs:192-195), quindi due modifiche su cue diverse non si fondono mai. Le opzioni sono una famiglia di varianti che emettono una splice unica sul range, o una voce di cronologia composita.

- **Cost today:** Un intervento in un crate ancora piccolo, i cui unici consumatori sono i suoi test e dodici comandi.
- **Cost if deferred:** M5 vuole applicare la resa approvata a tutte le righe segnalate e M6 vuole inserire una corrispondenza in tutte le occorrenze: entrambi si costruiscono la propria scappatoia a N comandi. L'utente ottiene una correzione QA che per essere annullata richiede quaranta Ctrl+Z, e a quel punto la macchina ha due consumatori chiusi da non rompere.

### 5. Il modello di selezione della griglia, prima che M2.5 lo dia per scontato. Oggi la selezione e un solo indice React che fa insieme da selezione e da linea attiva (src/components/CueList.tsx:85). Il criterio di M2.5 nomina gia play selection (BACKLOG.md:73) e il QA di M5 vorra selezionare tutte le righe segnalate.

- **Cost today:** Un cambio di stato in un componente unico, prima che tre funzioni ci si appoggino.
- **Cost if deferred:** M2.5 consegna una selezione che significa la riga attiva, M5 inventa il proprio concetto di insieme di righe segnalate, e i due non si riconciliano mai. Poi M7 li deve unificare a valle di entrambi.

### 6. Dove vive il motore di confronto testuale che serve sia a trova-e-sostituisci sia al QA termbase. CLAUDE.md sezione 4 vuole il core aperto pienamente utile da solo e vieta rami pro nel repo aperto; M5 e un modulo chiuso. Il confronto richiesto e lo stesso: trovare un termine sorgente in una riga ignorando i tag override.

- **Cost today:** Mettere il matcher e lo scanner dei tag ASS in un crate aperto e far consumare quello a M5. E una scelta di collocazione, non lavoro in piu.
- **Cost if deferred:** Due matcher con semantiche diverse, uno dei quali nel repo chiuso e quindi non riusabile, e un core aperto che non sa cercare. Il proprietario se ne accorge quando chiede la ricerca e gli viene detto che il motore c'e ma sta dall'altra parte del confine.

### 7. Se e come i sottotitoli vengono mostrati sul video, prima che M2.6 definisca il modello a due documenti. Oggi sono disattivati per configurazione (src-tauri/src/video/player.rs:41) e nulla chiama sub-add. Con due documenti aperti serve sapere quale dei due si vede.

- **Cost today:** Una nota di design dentro M2.6 e la riserva di un percorso di copia ombra nella cartella di lavoro.
- **Cost if deferred:** M2.6 consegna due documenti senza alcuna nozione di quale sia in scena, e aggiungerla significa rimettere le mani sul modello a due documenti appena finito. Peggio, si e tentati di ottenere l'anteprima ricaricando il file salvato, che significa sovrascrivere il file dell'utente a ogni tasto e mangiarsi l'anello dei backup.

### 8. Dove e con che tetto vive l'autosave, prima che l'autosave esista. Il tetto dei backup e dieci (crates/sublore-io/src/backup.rs:21), la potatura agisce per nome del file sorgente (crates/sublore-io/src/backup.rs:208-216), e ogni sovrascrittura archivia una copia (crates/sublore-io/src/atomic.rs:59-74). Un timer che passasse dallo stesso store cancellerebbe le copie di sicurezza dell'utente in dieci scatti.

- **Cost today:** Uno spazio separato, una convenzione di nome che la potatura dei backup non veda, e una frase di policy sulla ritenzione.
- **Cost if deferred:** Una regressione di sicurezza dei dati contro CLAUDE.md sezione 3.3, scoperta dall'utente nel momento in cui gli serve un backup e non c'e piu. E il tipo di bug che il progetto esiste per non avere.

### 9. Il gate delle modifiche non salvate alla chiusura della finestra. Oggi la gestione di CloseRequested spegne solo trascrizione e video (src-tauri/src/lib.rs) e non esiste alcun prevent_close nel repo; lo stato sporco e tracciato sia lato backend sia lato frontend, e nessuno lo consulta alla chiusura. Chiudere la finestra con edit non salvati li butta via in silenzio, contro CLAUDE.md sezione 3, che dice che un bug puo costare fastidio e mai dati. Il plugin di dialogo e gia una dipendenza.

- **Cost today:** Intercettare l'evento, chiedere, e rispettare la risposta. Poche ore, e il pezzo piu grosso e gia in casa.
- **Cost if deferred:** Resta l'unico percorso di perdita dati del prodotto per tutta la durata del piano attuale, e con M2.6 raddoppia perche i documenti aperti diventano due. Non e una funzione mancante: e un difetto attivo, e va corretto adesso, non messo in coda.

### 10. Se sublore-edit imparera mai a scrivere segmenti non-cue. Oggi l'enum Edit copre solo le cue (crates/sublore-edit/src/plan.rs:28-58) e la verifica costruisce le sue attese sulle cue, mentre le righe Style:, le proprieta dello script e gli allegati viaggiano come metadati non interpretati (crates/sublore-formats/src/document.rs:81-92). Ogni scrittura futura fuori dalle cue passa da qui.

- **Cost today:** Guardare onestamente se la forma di Edit e di Expectation puo crescere una variante Meta, mentre il crate e piccolo e i suoi soli consumatori sono i test. Anche solo scrivere la risposta e utile.
- **Cost if deferred:** Il crate avra i consumatori di M5 e M6 da non rompere, e la seconda via di scrittura si affianchera alla prima invece di sostituirla. M14 diventa grande per un motivo che poteva costare mezza giornata.

### 11. Se il prodotto parlera mai in fotogrammi. Non esiste alcuna nozione di framerate, fotogramma o keyframe nel codice di produzione, e nessuno dei tre formati di v1 e basato sui fotogrammi: SRT e VTT sono in millisecondi, ASS in centisecondi. Il player osserva solo la posizione temporale e la espone in secondi.

- **Cost today:** Una riga di risposta. Se e no, si smette di progettare intorno a un motore VFR e gli agganci di M2.5 restano in millisecondi senza scuse; se e si un giorno, si sa che la giuntura sta nel player e non si costruisce niente che la escluda.
- **Cost if deferred:** Si continua a scrivere nei piani che gli agganci sono approssimati in attesa di un motore che nessuno ha deciso di fare, e ogni voce di timing porta un blocker fantasma che gonfia le stime senza descrivere lavoro reale.

### 12. Se M2.4 costruisce il proprio provider audio invece di riusare quello della trascrizione. La funzione di estrazione dell'ASR e privata, scrive in una cartella di lavoro che si cancella da sola alla fine della corsa (crates/sublore-asr/src/scratch.rs:88-91) e produce mono a 16 kHz (crates/sublore-asr/src/sidecar.rs:285, :302-304), perche e tarata su whisper. Per i picchi va bene, per riprodurre un intervallo o esportare uno spezzone e audio degradato, e la durata di vita e sbagliata: per corsa e non per episodio.

- **Cost today:** Dire nel piano di M2.4 che si riusa il modello (scoperta di ffmpeg, esecuzione in background, progresso, annullamento) e non il codice, e prevedere una API pubblica con cache per episodio.
- **Cost if deferred:** M2.5 scopre a meta che i comandi di riproduzione a intervallo suonano un audio mono a 16 kHz, o che il file e sparito perche la corsa e finita, e il provider va rifatto mentre la waveform ci gira sopra.

### 13. Come nasce il file di traduzione di un episodio. Non esiste un comando per creare un documento nuovo, e salva-con-nome scrive una copia altrove lasciando la sessione puntata al file originale e ancora sporca, per scelta dichiarata (src-tauri/src/subtitle/mod.rs:390-391). Chi riceve solo il sorgente non ha un percorso pulito per produrre il target.

- **Cost today:** Decidere dentro M2.6, che gia apre due documenti, se il target puo nascere vuoto o come copia del sorgente, e cosa significa salvare la prima volta.
- **Cost if deferred:** Il traduttore ci arriva da solo passando dal rifiuto per modifiche non salvate e scegliendo scarta sul file sorgente, cioe da una finestra che gli dice che sta buttando via il proprio lavoro. E il primo gesto della sua giornata.

## What we will not do

An honest exclusion list is worth more than a plan that promises everything.

- **Karaoke in ogni forma: unione con tag di sillaba, divisione per sillabe, kanji timer, controller di timing per sillabe, e il modello dei tag di sillaba che li regge.** — Non obiettivo dichiarato in CLAUDE.md sezione 1 e in parcheggio esplicito in BACKLOG.md:129. Il proprietario ha riaperto il typesetting, che e cosa diversa: il karaoke resta fuori finche non lo riapre lui. Senza modello dei tag di sillaba non c'e nemmeno un substrato su cui costruirlo.
- **Motore di estensioni e automazione scriptabile.** — Escluso per decisione del proprietario. Lo registro qui perche conta nel valutare quanto Aegisub sia sostituibile: molti flussi fansub dipendono da script della comunita, e chi ci lavora sopra non migra per le funzioni di questo piano. Non e un buco tecnico nostro, e una quota di mercato che non inseguiamo.
- **Timecode VFR esterni, mappa fotogramma-tempo, e tutto il vocabolario che parla in fotogrammi: salto al keyframe successivo, confini di riga al fotogramma esatto, colonne dei tempi in fotogrammi, formato SMPTE.** — Nessuno dei tre formati di v1 e basato sui fotogrammi, e nel codice non esiste alcuna nozione di framerate. Costruire un motore VFR significa costruire un sottosistema per servire consumatori che non abbiamo. L'unica cosa che ne salviamo, i keyframe per l'aggancio alla scena, sta in M15 e non richiede il resto.
- **I sette formati legacy e broadcast di Aegisub: SSA in sola scrittura degradata, MicroDVD, TTXT, TXT con separatori, EBU STL, Adobe Encore, TranStation.** — CLAUDE.md sezione 1 fissa lo scope a SRT, ASS e VTT. Di quel dominio ci interessa il modello, cioe l'idea che ogni formato dichiari cosa non regge e degradi in modo esplicito, e quello entra in M13. I formati in se sono nicchia broadcast o archeologia.
- **Allineamento di una riga cercando un pixel colorato sui fotogrammi, e cattura del solo strato dei sottotitoli su sfondo trasparente.** — Il primo richiede i pixel grezzi dei fotogrammi con seek esatto, e CLAUDE.md sezione 2 vieta percorsi di decodifica paralleli. Il secondo richiede di invocare libass fuori da mpv, cioe una dipendenza nativa e una pipeline di rasterizzazione offscreen che non esistono. Entrambi servono a impaginare cartelli: e il pubblico piu lontano dal nostro.
- **Piu finestre applicative indipendenti, e video staccato in una finestra propria.** — La sessione di editing e una sola per costruzione (src-tauri/src/subtitle/mod.rs:33-40, la sessione dietro un mutex) e tutti i dodici comandi ne dipendono; la superficie e il player nascono legati alla finestra principale e la maniglia passata a mpv e fissata all'avvio. Aprire una seconda finestra significa identita di documento su tutta la superficie IPC piu un secondo player. Nel nostro flusso, cambiare episodio dentro la stessa finestra copre quasi tutto il bisogno reale, e M2.6 copre il resto.
- **Ricampionamento della risoluzione dello script, e in generale il classificatore semantico completo dei parametri dei tag override.** — Serve a portare typesetting fra risoluzioni diverse, che e un problema di chi impagina, non di chi traduce. Il costo e il classificatore completo, che e il pezzo piu grosso del modello dei tag; lo scanner minimo che ci serve davvero, quello che localizza i blocchi e rimappa gli offset, entra in M8 e costa una frazione.
- **Ritaglio vettoriale, disegni ASS, e qualunque editor di forme.** — Richiedono insieme il percorso video sostituito di M16 e un modello dei disegni che non esiste. Aegisub stessa delega il disegno a un eseguibile esterno. Se un giorno servisse, e contenuto di M16 e non prima.
- **Raggruppamento e collasso di righe nella griglia.** — E organizzazione di comodo, non una funzione che sblocca lavoro. La griglia e virtualizzata con aritmetica a riga fissa, quindi i gruppi romperebbero la mappa indice-posizione e imporrebbero un livello di indirezione; e lo stato andrebbe persistito nel database di progetto, perche sporcare il file dell'utente per stato di interfaccia va contro lo spirito di CLAUDE.md sezione 3.
- **Spettrogramma come modalita alternativa alla waveform.** — Serve al timer che deve distinguere l'attacco di una consonante sotto la musica. Per chi traduce, la waveform basta. Il costo non e la trasformata, che e facile, ma la cache dei blocchi e la mappatura delle frequenze. Rientra solo se il pubblico dei timer diventa un obiettivo dichiarato.
- **Comandi specifici di macOS e finestre multiple di sistema, e l'aggiornamento automatico.** — macOS e differito per decisione del proprietario, e l'auto-updater e in parcheggio esplicito (BACKLOG.md:129). Il controllo aggiornamenti esplicito da menu resta ammesso da CLAUDE.md sezione 1, ma richiede un canale di rilascio pubblico che oggi non esiste: quando ci sara, costa poche ore.
- **Catena di filtri configurabile all'export.** — L'idea buona, cioe l'export come pipeline dichiarata e riproducibile, appartiene al modulo batch, che e chiuso per CLAUDE.md sezione 4 e in parcheggio per BACKLOG.md:129. Costruirla nel core aperto significherebbe costruire l'impalcatura di una funzione che vivra altrove. La conversione semplice fra i nostri tre formati, che e cio che serve davvero, sta in M13.
