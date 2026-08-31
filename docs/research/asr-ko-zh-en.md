# ASR per coreano, cinese e inglese: cosa si può davvero mettere nel prodotto

Ricerca desk, chiusa il 2026-08-30. Companion di `asr-anime.md`, stesso metodo e stessi limiti:
nessun modello è stato scaricato, convertito o eseguito, su Linux o altrove. Tutto quello che segue
viene da model card, technical report, file di benchmark, config JSON, header binari letti da remoto,
issue tracker e sorgenti. Ogni numero è la misura di qualcun altro. Dove una cosa non è verificata,
o dove due fonti si contraddicono, è scritto.

Materiale di riferimento: animazione e serie in coreano, donghua e animazione in mandarino,
animazione originale e doppiaggi in inglese. Le difficoltà sono le stesse ovunque: parlato sopra
musica ed effetti, recitazione urlata, sussurrata e stilizzata, nomi e terminologia inventati, canto
che sborda nel dialogo, voci sovrapposte, e timestamp che devono essere giusti alla granularità
della battuta, non del paragrafo.

---

## 1. Una risposta per lingua

### Coreano: niente batte un buon Whisper large-v3, e non è vicino

Detto piatto: **per il coreano non esiste oggi nulla di spedibile che batta `whisper-large-v3`**, e
la ragione non è che i fine-tune coreani siano deboli, è che nessuno di quelli trovati sopravvive a
una lettura seria. Sono stati esaminati sette candidati coreani specifici e tutti e sette sono
caduti, per uno di tre motivi, spesso per più di uno: **licenza assente o non commerciale**
(`ghost613` e `o0dimplz0o` non dichiarano nessuna licenza, quindi non c'è concessione di
ridistribuzione; la famiglia `Sky-Kim/crisper-whisper2-*-ko` eredita per proprio NOTICE.md la licenza
non commerciale di Nyra Health più i termini AI-Hub); **numeri che non reggono l'ispezione** (il
vantaggio dichiarato dei `komixv2` viene per intero da due colonne su otto, e quelle due colonne sono
KsponSpeech, che compare nel loro stesso elenco di dati di addestramento; il "16 WER contro 24" di
`royshilkrot` risulta, leggendo il `trainer_state.json` che l'autore ha caricato nel repo, essere il
fine-tune allo step 500 contro sé stesso allo step 10000, mai contro il modello base);
**incompatibilità reale con il runtime** (il checkpoint CrisperWhisper coreano ha `vocab_size` 51896,
e whisper.cpp deriva tutti i token di controllo aritmeticamente da `n_vocab`, quindi caricherebbe con
ogni token speciale spostato di 31 posizioni, cioè timestamp sbagliati di circa 0,62 s senza nessun
errore visibile).

C'è un secondo motivo, più profondo e più utile da tenere a mente per il futuro: i corpora coreani
pubblici su cui questi modelli sono addestrati sono il posto sbagliato da cui partire per un
sottotitolatore. Zeroth è news lette in studio; Junhoee è telefonia da call center; KsponSpeech è
conversazione in stanza silenziosa con trascrizione verbatim dei riempitivi. Sulle 100 righe di test
Zeroth ispezionate durante il controllo, 0 su 100 contengono punteggiatura, 0 su 100 contengono una
cifra araba, e 35 su 100 scrivono i numeri a lettere separate da spazi (`이천 십 팔 년` per 2018).
Un fine-tune su quel materiale insegna al modello a smettere di punteggiare e a scrivere le date in
hangul spaziato. Per una battuta di sottotitolo è lavoro in più, non in meno, ed è esattamente il
contrario di quello che il coreano richiede.

Sulla **spaziatura**, che è la classe d'errore caratteristica: l'evidenza migliore è KEBAP
(EMNLP 2023, misurata, <https://aclanthology.org/2023.emnlp-main.292.pdf>), dove la spaziatura è la
categoria di errore testuale più grande, 514 istanze pari al 14,62%, davanti a punteggiatura
(14,37%) e numerali (14,22%). Nella stessa tabella Google, Clova e Whisper stanno tutti intorno a
0,5 WER e intorno a 0,2 CER **sullo stesso audio**: quel divario è quasi tutto confini di parola.
Il campo aggira il problema misurando in CER invece che in WER, e OpenAI lo dice esplicitamente nella
release note di large-v3, che il coreano è stato valutato in CER perché nelle label di Common Voice
e FLEURS le spaziature sono incoerenti. Conseguenza operativa: un CER coreano di 5 che sembra
eccellente può ancora significare una riga da rispaziare a mano. Il paper meteorologico coreano
arXiv 2410.18444 quantifica l'effetto: 8,68 CER / 22,58 WER / 12,50 WER space-normalised sullo stesso
set, cioè normalizzare la sola spaziatura cancella circa dieci punti di WER. Quando Sublore misurerà
il coreano sulle proprie fixture deve riportare **CER e sWER insieme**, e un diff di sola spaziatura,
altrimenti scarterà un modello per una convenzione ortografica invece che per un errore di
riconoscimento.

Sui **livelli di cortesia e le desinenze onorifiche**: nessun modello lo pubblicizza, nessun
benchmark lo misura. KEBAP è l'unica fonte che li nomina come classe misurabile, sotto "Remove"
(desinenze o suffissi omessi, 3,47%) e "Addition" (postposizione o desinenza non pronunciata
aggiunta, 2,96%). Circa il 6,4% degli errori annotati è a livello di desinenza, e per un traduttore
quello è danno di registro: una battuta in 반말 che torna in 존댓말 cambia chi è il personaggio.
Questa è una lacuna reale e non colmata, e con ogni probabilità è il posto dove una fixture coreana
di Sublore vale più di qualunque altra ricerca di modello.

Quindi: coreano su `whisper-large-v3` in whisper.cpp, con post-processing deterministico della
spaziatura e con il termbase a fare il lavoro pesante sui nomi. Su `large-v3` contro
`large-v3-turbo` non c'è evidenza affidabile in nessuna delle due direzioni: l'unica tabella che
suggeriva un crollo di turbo sul coreano spontaneo è la stessa la cui interpretazione non ha retto al
controllo, mentre una misura terza indipendente (la Space `baryonlabs/ko-asr-arena`, CER su
`kresnik/zeroth_korean`) dà turbo a 12,6 contro whisper-small a 18,0. È una domanda da risolvere con
un A/B sulle proprie fixture, non da decidere leggendo card.

### Cinese: qui sì, qualcosa batte Whisper, ma il migliore non gira in whisper.cpp

Il cinese è l'unica delle tre lingue dove la risposta onesta è "sì, e di parecchio". Due modelli
sono chiaramente avanti sul mandarino nel 2026, e ciascun laboratorio mette sé stesso primo sul
proprio harness: FireRedASR2-LLM dichiara 2,89% CER medio sui quattro set mandarini standard,
Qwen3-ASR-1.7B dichiara WenetSpeech 4,97 / 5,88 e SpeechIO 2,88, e il paper FireRed misura Qwen a
3,76% medio. Entrambi mettono `whisper-large-v3` a circa tre volte il loro errore (9,86% CER medio,
19,11% su WenetSpeech-Meeting). **Nessuno dei due gira in whisper.cpp**, e questo è tutto il
problema.

Dentro l'architettura esistente la risposta c'è ed è buona:
[`BELLE-2/Belle-whisper-large-v3-zh-punct`](https://huggingface.co/BELLE-2/Belle-whisper-large-v3-zh-punct),
Apache-2.0, architettura Whisper pura, quantizzazioni ggml di terzi già pubblicate
(<https://huggingface.co/uosx/Belle-whisper-large-v3-zh-punct-ggml>, q5_0 1081 MB), che circa dimezza
il CER mandarino di Whisper sui set pubblici nominati (AISHELL-1 8,085 → 2,945, WenetSpeech-Meeting
20,15 → 10,973) **e punteggia nativamente**, al costo di 0,02-0,75 punti CER rispetto al gemello non
punteggiato. La punteggiatura non è un dettaglio: nella famiglia dei toolkit cinesi (FireRedASR2,
Fun-ASR, Paraformer) la punteggiatura è un secondo modello nella pipeline, e per un sottotitolatore
"la punteggiatura è un altro modello" è un costo vero. Belle è la raccomandazione più chiaramente
spedibile di tutto questo documento.

**Semplificato contro tradizionale** è un problema reale, documentato e non risolto, e non va trattato
come una nota a piè di pagina. Whisper è stato addestrato su un misto dei due e la scelta di script è
di fatto arbitraria per file, con thread aperti da anni su openai/whisper (#277, #987), whisper.cpp
(#1450, #2318), faster-whisper (#521) e Subtitle Edit (#6830). Il workaround di comunità è un
`initial_prompt` nello script bersaglio (il default zh di whisper.cpp è 以下是普通話的句子。) ed è
riportato come inaffidabile. Per un tool che deve essere verificabile, "probabilistico" non basta:
lo script deve essere una **proprietà del progetto**, ottenuta o con un modello impegnato su uno
script (Belle per il semplificato,
[`MediaTek-Research/Breeze-ASR-25`](https://huggingface.co/MediaTek-Research/Breeze-ASR-25) per il
tradizionale e il mandarino di Taiwan, Apache-2.0, con conversione ggml di terzi già pubblicata) o
con un passaggio OpenCC deterministico in uscita, esposto come impostazione. Non con un prompt e una
speranza.

**Cantonese**: oggi funziona, e due anni fa non era vero. Il merito è del corpus WenetSpeech-Yue
(21.800 h, set 2025) e del benchmark WSYue-eval. Numeri su WSYue short/long: FireRedASR2-LLM
5,14/8,71, Qwen3-ASR 5,82/8,85, Doubao-ASR 10,51/11,39. Il punto che conta per Sublore è che
**Whisper-medium non compilato crolla a 80,41/80,82 CER sul cantonese**, cioè non è debole, è
inutilizzabile, e il fine-tune fa tutto il lavoro. Se il cantonese serve, dentro whisper.cpp
l'opzione è `Whisper-m-Yue` dentro [`ASLP-lab/WSYue-ASR`](https://huggingface.co/ASLP-lab/WSYue-ASR)
(Apache-2.0, fine-tune di whisper-medium, quindi convertibile), con un caveat non risolto: il
pipeline di preparazione dati del repo menziona rimozione della punteggiatura e conversione
semplificato→tradizionale, ma gli esempi di inferenza non dicono cosa i checkpoint emettano
davvero. Da verificare prima di fidarsi.

### Inglese: no, niente batte un buon Whisper large abbastanza da giustificare un cambio di modello

Detto piatto, perché è la risposta: **per l'inglese non cambiare modello**. Sul long-form, che è la
metrica che conta per i sottotitoli e non il read speech, il paper della Open ASR Leaderboard
(arXiv 2510.06961v4, 2026-03-30, 86 sistemi, 12 dataset) dà: ElevenLabs Scribe v2 7,32 (chiuso),
AssemblyAI Universal 3 Pro 8,34 (chiuso), Cohere Labs Transcribe 9,73 (aperto), Parakeet TDT 0.6B v3
10,7, Whisper large-v3-turbo 11,0, Canary-Qwen-2.5B 11,2, Whisper large-v3 11,2. I dataset long-form
sono CORAAL, Earnings21, Earnings22, TED-LIUM v3. Il vantaggio del miglior modello aperto su Whisper
è **mezzo punto di WER**, non una generazione, e i numeri da 5,4 che circolano (Cohere 5,42,
Canary-Qwen 5,63) vengono da segmenti corti e puliti e non trasferiscono. Aggiungi che su EdAcc
(40+ accenti inglesi) e AfriSpeech-OOD `whisper-large-v3` risulta primo, e che le varianti distillate
degradano di più sugli accenti: per doppiaggi con cast internazionale questo argomenta per restare su
large-v3, non per lasciarlo.

Dove invece c'è un guadagno grosso e gratis è **la configurazione, non il modello**. La più grande
fonte d'errore inglese su questo materiale non è l'accuratezza delle parole, è Whisper che inventa
righe sopra il non-parlato: arXiv 2501.11378 (2025-01-20) ha passato `whisper-large-v3` su 301.317
clip di non-parlato da AudioSet, MUSAN, UrbanSound8K e FSD50K e ha ottenuto allucinazioni sul 40,3%,
con loop sul 10-21%. L'animazione è piena esattamente di questo: stinger musicali, letti di effetti,
ambienze, silenzio sotto i titoli. Le uscite tipiche sono boilerplate da fansub ("thanks for
watching") che Whisper ha imparato dallo scraping. Il rimedio è già dentro il runtime: `--vad` con il
Silero VAD ggml incluso (MIT, ~864 KB), che chiude il decoder prima che veda il non-parlato. Che il
VAD risolva questa classe è meccanismo più consenso di comunità, non un end-to-end misurato: la
dimensione del guadagno va misurata sulle fixture di Sublore.

Il **canto** non lo risolve nessuno. Il miglior risultato aperto misurato su Jam-ALT è 20,35 WER
(arXiv 2506.15514, workshop ICME 2025), e perfino trascrivere uno stem vocale isolato pulito arriva a
14,19-14,98. La separazione con Demucs aiuta sui mix reali (MUSDB-ALT 23,59 → 20,00) e non fa niente
su Jam-ALT (20,99 → 21,08). L'inglese cantato è un problema al 15-20% di WER qualunque cosa gli si
metta davanti: il prodotto va progettato attorno al fatto che le canzoni le riscrive il traduttore,
non attorno alla speranza che il modello ce la faccia.

L'unico sfidante credibile resta [`nvidia/parakeet-tdt-0.6b-v3`](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3)
(CC-BY-4.0, 25 lingue europee, quindi inglese sì e coreano/cinese no), e il suo argomento **non è**
il mezzo punto di WER: è che è un modello TDT senza decoder autoregressivo, quindi strutturalmente
non può fare il loop di ripetizione di Whisper, punteggia e capitalizza nativamente, ed emette durate
per token con `word_start` e **confidenza per parola**, che Whisper non dà. Una parola a bassa
confidenza è una battuta da segnalare nell'editor: è un segnale di prodotto, non solo di accuratezza.
Cosa si perde: non esiste l'equivalente di `initial_prompt`, quindi si perde il trucco di innescare
nomi propri e terminologia inventata, che su animazione e doppiaggi potrebbe pesare più del guadagno
sui timestamp. È un compromesso da misurare, non da decidere da model card.

Il modello che davvero batterebbe large-v3 per i sottotitoli, CrisperWhisper 2.0, con 29,6 ms di
errore medio sui confini di parola e una robustezza al rumore molto migliore di WhisperX, è sotto
Nyra Health Non-Commercial Research License. Sublore si vende. È morto, e va registrato come morto
una volta sola per non riscoprirlo ogni sei mesi con entusiasmo.

---

## 2. La tabella

Tipo di evidenza: **misurata** = benchmark con numero e dataset nominato, terzo o paper;
**autore** = misura pubblicata dagli autori sul proprio modello; **comunità** = impressione o
report d'uso; **non verificata** = nessuna fonte primaria raggiunta.

| Modello                                                                                                        | Lingua                                   | Cos'è                                                                                                                                                                                        | Licenza                                                                                                                       | Evidenza (tipo)                                                                                                                                                                                                                                                                                                     | whisper.cpp                  | Verdetto                                                                   |
| -------------------------------------------------------------------------------------------------------------- | ---------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------- | -------------------------------------------------------------------------- |
| [openai/whisper-large-v3](https://huggingface.co/openai/whisper-large-v3)                                      | multi                                    | La baseline. Encoder-decoder 1,55B, ggml ufficiale, `initial_prompt`, timestamp DTW con preset di alignment head.                                                                            | Apache-2.0 (pesi), MIT (codice)                                                                                               | **Misurata**: long-form EN 11,2 WER a RTFx 68,6 (arXiv 2510.06961v4). Primo su EdAcc e AfriSpeech-OOD fra i modelli aperti. KEBAP: 0,48/0,67/0,92 WER e 0,23/0,35/0,65 CER coreano per difficoltà.                                                                                                                  | Nativo                       | **Default per KO ed EN**                                                   |
| [openai/whisper-large-v3-turbo](https://huggingface.co/openai/whisper-large-v3-turbo)                          | multi                                    | Distillazione a 4 layer di decoder, ~2,2x più veloce. Preset DTW presente in `whisper.h`.                                                                                                    | MIT                                                                                                                           | **Misurata**: long-form EN 11,0 a RTFx 148, marginalmente meglio di large-v3. Coreano su zeroth: 12,6 CER (Space `baryonlabs/ko-asr-arena`). Degrada più di large-v3 sugli accenti.                                                                                                                                 | Nativo                       | Da A/B contro large-v3                                                     |
| [BELLE-2/Belle-whisper-large-v3-zh-punct](https://huggingface.co/BELLE-2/Belle-whisper-large-v3-zh-punct)      | cinese semplificato                      | Fine-tune large-v3 per mandarino con punteggiatura distillata da CT-Transformer. ggml di terzi già pubblicato.                                                                               | Apache-2.0                                                                                                                    | **Autore** su set pubblici nominati: AISHELL-1 2,945, AISHELL-2 3,808, WenetSpeech net 8,998 / meeting 10,973, HKUST 17,196, contro large-v3 8,085 / 5,475 / 11,72 / 20,15 / 28,597.                                                                                                                                | Conversione                  | **Raccomandato per ZH**                                                    |
| [BELLE-2/Belle-whisper-large-v3-turbo-zh](https://huggingface.co/BELLE-2/Belle-whisper-large-v3-turbo-zh-ggml) | cinese semplificato                      | Stessa famiglia in classe turbo, con ggml ufficiale BELLE-2 (1625 MB).                                                                                                                       | Apache-2.0                                                                                                                    | **Autore**: AISHELL-1 3,070 contro 8,639 della baseline turbo, WenetSpeech-Meeting 13,357. Nessuna data di rilascio trovata.                                                                                                                                                                                        | Nativo (ggml pubblicato)     | Alternativa veloce                                                         |
| [MediaTek-Research/Breeze-ASR-25](https://huggingface.co/MediaTek-Research/Breeze-ASR-25)                      | cinese tradizionale                      | Fine-tune large-v2 per mandarino di Taiwan e code-switching zh-en, con timestamp dichiarati e allineamento migliorato per il captioning. ggml di terzi (`danielkao0421/Breeze-ASR-25-ggml`). | Apache-2.0                                                                                                                    | **Autore** su set pubblico: CSZS-zh-en 13,01 WER contro 29,49 di large-v2. Nessun CER su AISHELL o WenetSpeech, quindi non confrontabile con Belle. Dati cinesi **interamente sintetici TTS** (10.000 h).                                                                                                           | Conversione                  | Unica via tradizionale in-architettura                                     |
| [ASLP-lab/WSYue-ASR](https://huggingface.co/ASLP-lab/WSYue-ASR) (Whisper-m-Yue)                                | cantonese                                | Fine-tune whisper-medium sul corpus WenetSpeech-Yue. Insieme a Conformer-Yue, SenseVoice-s-Yue, Conformer-LLM-Yue.                                                                           | Apache-2.0                                                                                                                    | **Misurata** su WSYue-eval short/long CER: Whisper-m-Yue 8,51/5,05, SenseVoice-s-Yue 6,93/5,23. Whisper-medium non compilato: 80,41/80,82.                                                                                                                                                                          | Conversione                  | Unica opzione cantonese in-architettura                                    |
| [nvidia/parakeet-tdt-0.6b-v3](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3)                              | EN + 24 europee                          | FastConformer-TDT 600M, punteggiatura e maiuscole native, durate per token, confidenza per parola. GGUF ggml-org: f16 1256 MB, q4_k 416 MB.                                                  | CC-BY-4.0 (richiede attribuzione in-app)                                                                                      | **Misurata**: long-form EN 10,7 a RTFx 1000, miglior modello aperto dopo Cohere. **Autore**: LibriSpeech test-clean 1,93, FLEURS EN 4,85; con MUSAN a SNR 0 sale a 11,66 e a SNR -5 a 19,88.                                                                                                                        | Sì, ma vedi §5               | Candidato secondo sidecar EN                                               |
| [Qwen/Qwen3-ASR-1.7B](https://huggingface.co/Qwen/Qwen3-ASR-1.7B)                                              | multi                                    | Encoder audio + decoder Qwen3. 30+ lingue, 22 dialetti cinesi, cantonese di prima classe. Nessun timestamp: serve l'allineatore separato.                                                    | Apache-2.0                                                                                                                    | **Autore/report** (arXiv 2601.21337): WenetSpeech 4,97/5,88, AISHELL-2 2,71, SpeechIO 2,88, CV-yue 7,57, FLEURS-yue 3,98, M4Singer 5,98 WER, EntireSongs-en 14,60, ExtremeNoise 16,17. Sul coreano vedi §5: due letture dello stesso report non concordano.                                                         | No                           | Solo con secondo runtime                                                   |
| [Qwen/Qwen3-ForcedAligner-0.6B](https://huggingface.co/Qwen/Qwen3-ForcedAligner-0.6B)                          | 11 lingue, incl. KO, ZH, YUE, EN         | Allineatore forzato non autoregressivo a riempimento di slot, fino a ~5 min di audio. Prende (audio, testo) da qualunque trascrittore.                                                       | Apache-2.0                                                                                                                    | **Autore/report**, Accumulated Average Shift: media 42,9 ms contro NFA 129,8 e WhisperX 133,2; su audio concatenato a 300 s 52,9 ms contro NFA 246,7 e WhisperX 2708,4. Per lingua: ZH 33,1, EN 37,5, KO 37,2. Nessuna replica indipendente.                                                                        | No                           | Migliore opzione timing, costo runtime                                     |
| [FireRedTeam/FireRedASR2](https://github.com/FireRedTeam/FireRedASR2S)                                         | cinese                                   | ASR + VAD + LID + punteggiatura separati. Lirica cantata come task di prima classe.                                                                                                          | Apache-2.0                                                                                                                    | **Autore** (arXiv 2603.10420, 2026-03-11): mandarino 2,89% CER medio, opencpop 1,12%, WSYue 5,14/8,71, dialetti 11,55%. **Nessuna capacità di timestamp documentata**.                                                                                                                                              | No                           | Escluso: senza timestamp non serve                                         |
| [FunAudioLLM/SenseVoiceSmall](https://huggingface.co/FunAudioLLM/SenseVoiceSmall)                              | zh, yue, en, ja, ko                      | 234M CTC, tag di evento audio incluso `<BGM>`, ITN opzionale. GGUF da 893 MB a 139 MB. Cap ~30 s per chiamata.                                                                               | **Custom** "FunASR Model Open Source License Agreement", non OSI, con clausola di condotta e diritto unilaterale di revisione | **Misurata da terzi** (transcribe.cpp, con IC al 95% e comando di riproduzione): LibriSpeech test-clean 3,13% F32, FLEURS-zh CER 10,11% Q8_0 contro 10,20% del riferimento FunASR.                                                                                                                                  | No                           | Bloccato dalla licenza                                                     |
| [nvidia/nemotron-3.5-asr-streaming-0.6b](https://huggingface.co/nvidia/nemotron-3.5-asr-streaming-0.6b)        | multi (incl. KO, ZH, EN)                 | FastConformer-RNNT 600M streaming, punteggiatura nativa, chunk 80-1120 ms.                                                                                                                   | OpenMDW-1.1 (permissiva, non ancora OSI-approvata)                                                                            | **Autore**: FLEURS coreano da 7,59 a 7,12 CER a seconda del chunk. Contro Qwen 2,57 e Whisper 3,72 sullo stesso set non è competitivo sul coreano.                                                                                                                                                                  | No                           | Storia streaming, non accuratezza                                          |
| [CohereLabs/cohere-transcribe-03-2026](https://huggingface.co/CohereLabs/cohere-transcribe-03-2026)            | multi (14 lingue)                        | 2B Conformer + decoder Transformer. Punteggia di default, controllabile da prompt. **Non emette timestamp**. Repo gated.                                                                     | Apache-2.0                                                                                                                    | **Misurata**: EN short-form 5,42 WER (primo), long-form 9,73 a RTFx 418, LibriSpeech Clean 1,25. Per-lingua pubblicato **solo come figura**: nessun numero coreano o cinese leggibile, e il file `.eval_results` contiene solo la eval inglese, quindi il dato coreano è **non pubblicato**, non nascosto dal gate. | No                           | Escluso: niente timestamp                                                  |
| [CrisperWhisper 2.0](https://huggingface.co/nyralabs/CrisperWhisper2.0_large)                                  | multi                                    | Architettura Whisper, timestamp di parola migliori di tutto il resto, robusti al rumore.                                                                                                     | **Nyra Health Non-Commercial Research License**                                                                               | **Misurata** (arXiv 2408.16589), F1 confine di parola a 0,2 s: pulito 84,7 contro WhisperX 76,7 e Whisper 74,7; con rumore 79,5 contro WhisperX 59,0 e Whisper 68,3.                                                                                                                                                | Conversione (ma irrilevante) | Bloccato dalla licenza                                                     |
| [Silero VAD](https://github.com/snakers4/silero-vad)                                                           | n/a                                      | Segmentazione parlato/non-parlato. ONNX, <1 ms per chunk da 30 ms su un thread, binding Rust di comunità. Già dentro whisper.cpp come `--vad`.                                               | MIT                                                                                                                           | **Misurata indirettamente**: il fallimento che risolve è quantificato (40,3% di allucinazioni su non-parlato, arXiv 2501.11378). Che il VAD lo elimini è meccanismo + **comunità**, non end-to-end misurato.                                                                                                        | Nativo                       | **Da attivare subito**                                                     |
| [whisper.cpp `-dtw`](https://github.com/ggml-org/whisper.cpp)                                                  | n/a                                      | Timestamp per token via DTW sulla cross-attention. Preset di alignment head per ogni taglia incluso large-v3 e turbo. Marcato `[EXPERIMENTAL]` in `whisper.h`.                               | MIT                                                                                                                           | **Misurata** per la classe: i timestamp da attention di Whisper reggono a F1 68,3 con rumore mentre WhisperX scende a 59,0 (arXiv 2408.16589). Classe di accuratezza ~100 ms. **Comunità**: Discussion #2307 riporta timing "consistently off" su base.en, mai risolta.                                             | Nativo                       | Da attivare e misurare                                                     |
| [Montreal Forced Aligner 3.0](https://github.com/MontrealCorpusTools/Montreal-Forced-Aligner)                  | KO, ZH (mandarino), EN                   | Allineatore HMM-GMM su Kaldi. Modelli acustici coreano, mandarino e inglese. **Nessun modello cantonese.**                                                                                   | MIT (tool), CC-BY-4.0 (modelli)                                                                                               | **Misurata, terza** (arXiv 2606.18466, 2026-06-16): coreano Seoul Corpus 14,78 ms contro KFA 22,34 e BFA 85,81; inglese TIMIT 12,11 ms a livello di fono. Il **cinese non è stato valutato** in quel paper.                                                                                                         | No (conda + Kaldi + Python)  | Al massimo workflow esterno                                                |
| [WhisperX](https://github.com/m-bain/whisperX)                                                                 | multi                                    | Allineamento forzato wav2vec2 sopra Whisper. Per KO usa `kresnik/wav2vec2-large-xlsr-korean`, per ZH `jonatasgrosman/...-chinese-zh-cn`. **Nessuna voce `yue`.**                             | BSD-2-Clause (verificato leggendo LICENSE, non snippet)                                                                       | **Misurata**: 110,04 / 110,90 ms di errore medio sui confini di parola (TIMIT / Buckeye) e 133,2 ms AAS; su audio concatenato a 300 s degrada a 2708,4 ms. I checkpoint KO e ZH sono fermi al 2022-2023.                                                                                                            | No                           | Sconsigliato per long-form                                                 |
| [ctc-forced-aligner / MMS-300M-1130](https://huggingface.co/MahmoudAshraf/mms-300m-1130-forced-aligner)        | multi                                    | L'allineatore più scaricato dell'ecosistema (2,5M download/30gg). Comparirà in qualunque pipeline copiata da internet.                                                                       | **CC-BY-NC-4.0**, bloccante. Il tool `ctc-forced-aligner` non dichiara licenza.                                               | **Misurata**: 43,06 / 49,54 ms (TIMIT / Buckeye), meglio di WhisperX, molto dietro a MFA.                                                                                                                                                                                                                           | No                           | **Trappola da evitare**                                                    |
| [whisper-timestamped](https://github.com/linto-ai/whisper-timestamped)                                         | multi                                    | Timestamp di parola con confidenza, stessa famiglia di metodo del `-dtw`.                                                                                                                    | **AGPL-3.0**                                                                                                                  | **Non verificata**: nessun benchmark indipendente trovato.                                                                                                                                                                                                                                                          | No                           | Da non vendorare: AGPL con moduli chiusi accanto è una domanda da avvocato |
| [Paraformer-large (FunASR)](https://huggingface.co/funasr/Paraformer-large)                                    | cinese                                   | Non autoregressivo, 220M, 60.000 h di mandarino, predice i timestamp come parte del riconoscimento. Hotword biasing, che mappa bene su un termbase.                                          | Apache-2.0 su HF, ma la distribuzione ModelScope ha storicamente avuto licenza diversa: controllare la copia scaricata        | **Autore**: 6,97% CER su WenetSpeech meeting; AAS 71,0 ms su AISHELL (arXiv 2301.12343). Solo parlato letto mandarino.                                                                                                                                                                                              | No                           | Pavimento di confronto                                                     |
| [Fun-ASR-Nano-2512](https://github.com/QwenAudio/Fun-ASR)                                                      | multi                                    | 800M LLM-based, GGUF ~484 MB. Punteggiatura non nativa; il repo dichiara che **i timestamp non sono affidabili**.                                                                            | Apache-2.0                                                                                                                    | **Autore**: AISHELL-1 1,80% CER, FLEURS-zh 2,56%. Su harness terzo (FireRed) 4,55% medio, dietro a entrambi i leader.                                                                                                                                                                                               | No                           | Escluso: timestamp inaffidabili                                            |
| [DataoceanAI/dolphin-small](https://huggingface.co/DataoceanAI/dolphin-small)                                  | 40 lingue orientali + 22 dialetti cinesi | E-Branchformer CTC-attention 372M, token a due livelli lingua/regione. Solo base e small pubblicati.                                                                                         | Apache-2.0                                                                                                                    | **Autore**: WER medio 25,2 su 40 lingue. Nessun CER cinese su set nominati, quindi non classificabile contro i leader. Punteggiatura non trattata.                                                                                                                                                                  | No                           | Nessun vantaggio che giustifichi un runtime                                |
| [Distil-Whisper large-v3.5](https://huggingface.co/distil-whisper/distil-large-v3.5)                           | inglese                                  | Whisper-architettura, converte pulito, leva di velocità più economica in-architettura.                                                                                                       | MIT                                                                                                                           | **Non verificata** sui numeri che circolano. **Misurata** la controindicazione: le distillate perdono più di large-v3 sui set ad accenti diversi.                                                                                                                                                                   | Conversione                  | Compromesso sbagliato per i doppiaggi                                      |
| [Return Zero / Naver Clova (API)](https://github.com/rtzr/Awesome-Korean-Speech-Recognition)                   | coreano                                  | Il soffitto commerciale coreano. Cloud, non spedibile in un prodotto locale.                                                                                                                 | Commerciale                                                                                                                   | **Misurata** (benchmark pubblicato dal vendor che lo vince): CER medio Return Zero 5,91, Clova 7,52, Whisper API 11,39, Gemini 2.0 Flash 16,58. Non datato.                                                                                                                                                         | No                           | Solo per calibrare cosa vuol dire "buono" in coreano                       |

### Coreano: candidati esaminati e scartati

Nessuno di questi va nel prodotto. Sono elencati perché sono i primi risultati che qualunque ricerca
coreana produce, e vanno respinti una volta in modo deliberato invece che riscoperti.

| Modello                                                                                                                                                          | Perché no                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| ---------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [seastar105/whisper-medium-komixv2](https://huggingface.co/seastar105/whisper-medium-komixv2) e [small](https://huggingface.co/seastar105/whisper-small-komixv2) | **Nessuna licenza dichiarata** in card, metadata o file, quindi nessuna concessione di ridistribuzione. Il vantaggio dichiarato (7,30 contro 7,99 CER medio) viene per intero da due colonne su otto: sulla tabella dello stesso autore large-v3 vince cinque set su otto, e togliendo le due colonne KsponSpeech large-v3 guida 5,99 contro 6,71. KsponSpeech (AI-Hub 123) è nel suo elenco di dati di addestramento. Il divario residuo è compatibile con l'apprendimento delle convenzioni di trascrizione verbatim di quel corpus, che per una battuta di sottotitolo è un anti-feature.                      |
| [Sky-Kim/crisper-whisper2-small-finetuned-ko](https://huggingface.co/Sky-Kim/crisper-whisper2-small-finetuned-ko)                                                | Due blocchi indipendenti. Licenza: il NOTICE.md dichiara "la più restrittiva dei componenti", cioè Nyra Health non commerciale più i termini AI-Hub. Runtime: `vocab_size` 51896, e whisper.cpp deriva `is_multilingual`, `num_languages` e l'offset dei timestamp da `n_vocab`, quindi ogni token di controllo uscirebbe spostato di 31 posizioni, con timestamp sbagliati di ~0,62 s senza errore visibile.                                                                                                                                                                                                     |
| [ghost613/whisper-large-v3-turbo-korean](https://huggingface.co/ghost613/whisper-large-v3-turbo-korean)                                                          | **Nessuna licenza**, LICENSE 404. Manca `generation_config.json`, quindi niente `alignment_heads`: il percorso timestamp DTW resta senza preset corretto. Le durate dichiarate nella card sono ~4x quelle ufficiali di Zeroth, e il set di eval reale è ~36 minuti da al massimo 10 speaker. Corpus di sole news lette, senza punteggiatura e con i numeri scritti a lettere.                                                                                                                                                                                                                                     |
| [royshilkrot/whisper-large-v3-turbo-korean-ggml](https://huggingface.co/royshilkrot/whisper-large-v3-turbo-korean-ggml)                                          | Apache-2.0 e ggml f16 valido (header verificato: magic ggml, `n_text_layer` 4, `n_mels` 128), quindi tecnicamente caricabile. Ma il "16 WER da 24" è, leggendo il `trainer_state.json` che l'autore ha caricato, `eval_wer` 23,74 allo step 500 contro 16,22 allo step 10000: nessuna eval allo step 0, quindi il modello base non è mai stato misurato. L'eval è ritagliata dal pool di training di un dataset che non ha split di test, su un corpus di 3.000 frasi lette da 105 persone: sovrapposizione testuale fra train ed eval quasi inevitabile. Il dataset a monte non dichiara licenza né provenienza. |
| [SungBeom/whisper-small-ko](https://huggingface.co/SungBeom/whisper-small-ko)                                                                                    | I pesi sul Hub sono un autosalvataggio a metà training ("Training in progress, epoch 0", 2023-06-23), e il WER 9,48 dichiarato è una riga di log a epoch 0,2 sulla partizione 1 di 5, cioè ~4% del corpus. Non descrive il file che si scarica. L'unico tentativo indipendente di misurarlo (`baryonlabs/ko-asr-arena`) fallisce il caricamento perché manca `generation_config.json`. Licenza Apache-2.0 dichiarata dall'uploader sopra dati AI-Hub i cui termini richiedono accordo separato per entità estere.                                                                                                 |
| [TeamUNIVA/qwen3_asr_1.7b_ko_beta](https://huggingface.co/TeamUNIVA/qwen3_asr_1.7b_ko_beta)                                                                      | Apache-2.0 e reale, ma: non è Whisper, non ha GGUF proprio, non emette timestamp (servirebbe l'allineatore separato), il 70% del set di eval è KsponSpeech, e lo scoring tiene il migliore fra riferimento TN e ITN per enunciato, cioè una selezione oracolo che deprime il CER assoluto di una quantità non dichiarata e lo rende non confrontabile con nessun numero pubblicato. Nessuna eval script, nessun set rilasciato, 55 download, etichettato beta.                                                                                                                                                    |

---

## 3. Timestamp e segmentazione

È la parte su cui vive un tool di sottotitoli, ed è anche la parte dove il consiglio di default della
comunità è misurabilmente sbagliato per questo materiale.

**WhisperX non è la risposta per il long-form.** Due gruppi indipendenti a sette mesi di distanza lo
misurano nella stessa fascia: 110,04 / 110,90 ms di errore medio sui confini di parola su TIMIT e
Buckeye (arXiv 2606.18466) e 133,2 ms di AAS (arXiv 2601.21337). Il problema vero non è quello: è
che su audio concatenato a 300 s l'AAS sale a 2708,4 ms. Gli episodi sono audio lungo. Quel
fallimento si vede come battute giuste all'inizio di una scena e in ritardo di secondi alla fine.
Nota di correttezza che vale la pena non nascondere: gli autori del paper MFA dicono loro stessi che
la tabella a livello di parola penalizza WhisperX, NFA, MMS e BFA perché emettono intervalli con
buchi fra parole invece di un confine condiviso, il che raddoppia i punti di confronto. Le tabelle a
livello di fono sono il confronto equo, e lì gli allineatori ASR non ci sono. Resta che due misure
indipendenti convergono, quindi la conclusione tiene anche scontando l'artefatto.

**Il soffitto di accuratezza è MFA e il pavimento di installabilità pure.** MFA 3.0 misura 14,78 ms
sul coreano (Seoul Corpus) e 12,11 ms sull'inglese (TIMIT) a livello di fono, battendo perfino
l'allineatore coreano specifico KFA (22,34 ms). Il costo è conda + Kaldi + OpenFST + pynini +
Python 3.11 più un dizionario di pronuncia per lingua. Non è una cosa che si incorpora in un sidecar
Tauri. Al massimo è un workflow esterno documentato per utenti avanzati. E per il cinese non esiste
misura in quel paper (ha coperto inglese, giapponese e coreano), mentre per il cantonese non esiste
proprio il modello acustico.

**L'opzione con i numeri migliori fra le cose incorporabili** è Qwen3-ForcedAligner-0.6B: Apache-2.0,
copre coreano, cinese, cantonese e inglese in un solo allineatore, 32-43 ms di AAS, e soprattutto
degrada pochissimo sul long-form (52,9 ms contro 42,9 ms) perché non è autoregressivo. Prende
(audio, testo) da qualunque trascrittore, quindi può stare **dopo** whisper.cpp senza sostituirlo.
Esistono già due port C++ ggml indipendenti, [CrispASR](https://github.com/CrispStrobe/CrispASR)
(MIT, build Vulkan, ma la sua doc dice che le build `-vulkan` non fanno fallback su CPU) e
[qwen3-asr.cpp](https://github.com/predict-woo/qwen3-asr.cpp) (MIT, Metal e CPU documentati, Vulkan
no), più quantizzazioni GGUF di comunità fino a 0,53 GB. Tutti e tre i caveat vanno detti: i numeri
sono autodichiarati e mai replicati da terzi, `qwen3-asr.cpp` dichiara di non aver verificato la
parità di accuratezza con l'implementazione di riferimento, e la conversione GGUF avverte che la
pre-tokenizzazione a spazi bianchi è subottimale per cinese e giapponese, dove sarebbe corretta
quella a carattere. Per il cinese quindi il timing "di parola" deriva da una tokenizzazione che non
corrisponde a come il cinese si scrive: da verificare alla granularità di battuta prima di fidarsi.

**Quello che si può fare oggi, dentro il runtime, a costo zero:** `-dtw` con i preset di alignment
head già presenti in `whisper.h` (verificati leggendo il sorgente, non la documentazione di terzi:
`WHISPER_AHEADS_LARGE_V3` e `WHISPER_AHEADS_LARGE_V3_TURBO` esistono, `cli.cpp` parsa `-dtw` e imposta
`cparams.dtw_token_timestamps`), più `--vad` con Silero. Il primo è marcato sperimentale e ha almeno
un report di comunità mai risolto che lo dà "consistently off"; il secondo attacca direttamente il
drift da lunghi vuoti non parlati che arXiv 2607.05364 misura (su Whisper-tiny: da 2752 ms a 223 ms
di errore su vuoti misti fuori dominio). Attenzione a un dettaglio che vale per tutti i fine-tune:
i preset di alignment head esistono solo per i checkpoint Whisper standard, e un fine-tune ha
bisogno che le sue teste siano ricavate di nuovo, altrimenti ricade su `WHISPER_AHEADS_N_TOP_MOST`.

**La segmentazione è la metà che tutti saltano.** Dove spezzare una battuta (velocità di lettura,
caratteri per riga, non staccare un articolo dal suo nome, preferire l'inizio di una proposizione) è
un problema diverso dal timing e nessuno lo risolve: i sistemi riempiono la riga fino al limite e
ignorano la sintassi. Due cose concrete. L'API di regrouping di
[stable-ts](https://github.com/jianfch/stable-ts) (`split_by_gap`, `split_by_punctuation`,
`split_by_length`) è la specifica de facto di questo comportamento e va letta prima di riscriverla in
Rust, ma il repo è archiviato dal 2026-05-30, quindi si legge e non si dipende. E
[SubER](https://github.com/apptek/SubER) (Apache-2.0, IWSLT 2022) è la metrica che valuta insieme
traduzione, segmentazione e timing su **file** di sottotitoli senza preallineare ipotesi e
riferimento: è la base giusta per un test di accettazione automatico del tipo che CONTRIBUTING.md §5
richiede, perché misura "i tempi e i tagli sono giusti" e non solo "le parole sono giuste".
Come riferimento di formato, NeMo Forced Aligner produce direttamente ASS con evidenziazione a
livello di parola, che è utile da guardare per capire come deve stare su disco un ASS con timing di
parola.

---

## 4. Cosa Sublore potrebbe spedire, e cosa costa

### Prima di tutto: l'ASR non è il baricentro, e una raccomandazione che lo rende tale è sbagliata

CONTRIBUTING.md §1 lo dice: la trascrizione Whisper è una commodity che avvolgiamo, il prodotto è la
memoria. Tutto quello che c'è sopra va letto con quella lente, e con quella lente la maggior parte
delle opzioni scompare. Un secondo runtime ASR non è un pomeriggio: è un secondo binario da buildare
per Linux e Windows, una seconda matrice Vulkan/CPU da provare, un secondo formato di modello da
scaricare, versionare e cachare, un secondo protocollo di sidecar, un secondo insieme di modi di
fallire, e un secondo posto dove il budget di §7 (cold start < 2 s, idle < 400 MB) può rompersi. Per
comprare cosa? Sul cinese mezza generazione di CER, che però Belle recupera in buona parte restando
dentro whisper.cpp. Sull'inglese mezzo punto di WER. Sul coreano niente di dimostrato.

Nel frattempo il termbase non esiste ancora, e il termbase è la cosa che si vende. La conclusione
onesta è che **la scelta ASR per la v1 è già presa e va lasciata stare**, con tre eccezioni piccole e
a basso costo elencate qui sotto, e che il budget di ingegneria va sul QA terminologico e sulla TM.
Un nome proprio inventato lo sbaglieranno tutti i modelli di questo documento: quello si risolve nel
termbase, non nell'ASR. Anzi, è precisamente il caso d'uso che giustifica il prodotto.

### Cosa spedire nella v1, in ordine di rapporto valore/costo

1. **Attivare `--vad` con Silero.** Costo: una flag e un modello da 864 KB già supportato, MIT.
   Attacca la classe d'errore più grande su questo materiale (40,3% di allucinazioni sul
   non-parlato). Vale per tutte e tre le lingue. Va misurato sulle fixture, non assunto.
2. **Attivare e misurare `-dtw`.** Costo: una flag, preset già presenti. È la risposta a costo zero
   sulla granularità di battuta, ed è più robusta al rumore di WhisperX secondo la misura citata.
   Marcato sperimentale a monte: va provato, non dato per buono.
3. **`Belle-whisper-large-v3-zh-punct` come default cinese.** Costo: una conversione ggml (o l'uso
   della quantizzazione di comunità già pubblicata, previa verifica), più un download in più. È
   Apache-2.0, dimezza il CER mandarino di Whisper e punteggia. Questa è la singola raccomandazione
   più chiaramente positiva del documento.
4. **Impostazione di progetto per lo script cinese**, con passaggio OpenCC deterministico. Costo:
   una dipendenza piccola e una preferenza nel file di progetto. Non risolvibile col prompting.
5. **Post-pass di spaziatura coreana**, e metriche coreane che riportano CER e sWER separatamente.
   Costo: lavoro di normalizzazione, zero modelli nuovi.
6. **Progettare l'interfaccia del sidecar in modo che il motore sia sostituibile.** Costo: quasi
   zero oggi se fatto adesso, molto se fatto dopo. Non è un impegno a cambiare motore, è la
   condizione per poterlo valutare senza riscrivere.

### Cosa non spedire nella v1

Un secondo sidecar (Qwen3-ASR o Parakeet). Se e quando, è un **milestone proprio**, con la sua matrice
di verifica su Linux e la sua compilazione su Windows, e va giustificato da una misura sulle fixture
di Sublore, non da una classifica. Il candidato più forte per quel milestone non è il modello con il
CER migliore, è la coppia Qwen3-ASR-1.7B più Qwen3-ForcedAligner-0.6B, perché è l'unica combinazione
Apache-2.0 che copre mandarino, cantonese, coreano e inglese e ha una storia di timing.

### La scelta del modello per lingua è UI e download, non solo modello

Questo va detto perché è la metà del costo e nessun benchmark la nomina. Nel momento in cui il
cinese vuole Belle, il cantonese vuole Whisper-m-Yue, l'inglese vuole large-v3 e il coreano vuole
large-v3, Sublore ha smesso di avere "un modello" e ha un **gestore di modelli**. Le cose che ne
discendono:

- Disco e primo avvio. Ogni large sta fra 1,0 e 1,6 GB in ggml quantizzato. Un utente che lavora su
  tre lingue ne scarica tre. Serve una UI di download con progresso, ripresa, checksum e cancellazione
  esplicita, e un comportamento sensato quando il modello non c'è ancora (CONTRIBUTING.md §1: la rete si
  tocca solo per download modello opzionale).
- Default per progetto, non globale. La lingua sorgente è una proprietà del progetto, e con essa lo
  script cinese (semplificato o tradizionale) e la variante (mandarino o cantonese). Sono impostazioni
  visibili all'utente, non euristiche.
- Attribuzione e licenze in-app. Parakeet è CC-BY-4.0: richiede attribuzione. Belle, Breeze e i
  modelli WSYue sono Apache-2.0: serve il NOTICE. Una schermata di licenze che elenca cosa è
  installato non è opzionale in un prodotto GPL-3.0 che scarica pesi di terzi.
- Verifica. Ogni modello aggiuntivo che si offre è una riga in più nella matrice di verifica
  comportamentale, su Linux oggi e su Windows quando quel milestone arriverà. Tre modelli linguistici
  significano tre set di fixture, non uno.

Questo è il vero costo marginale di ogni modello aggiuntivo, e va contato prima di aggiungerne uno,
non dopo.

---

## 5. Cosa nessuno ha misurato

Qui l'evidenza finisce, e questo elenco è la parte del documento più utile da tenere.

- **Non esiste nessun benchmark pubblico di ASR su audio di animazione**, in nessuna delle tre
  lingue. Non uno. Tutti i numeri sopra vengono da riunioni, earnings call, lezioni, telefonate,
  parlato letto e conversazione in stanza silenziosa. Nessuno, incluso questo documento, può dirti
  oggi se il vantaggio di mezzo punto di Parakeet sopravvive a una scena di combattimento con
  colonna sonora, o se si inverte.
- **Niente è addestrato su donghua o su animazione coreana.** L'unica cosa vicina è il corpus AI-Hub
  방송 콘텐츠 대화체 (dataSetSn=463), circa 1.000 h di parlato conversazionale broadcast con musica ed
  effetti da varietà e drama, che però è un dataset e non un modello, e i termini AI-Hub complicano
  qualunque cosa vi si addestri sopra. Sul cinese esiste CineDub-CN (200+ serie TV, ~4.700 h di
  parlato effettivo) ma è un dataset per doppiaggio e TTS: nessun checkpoint ASR pubblicato sopra.
- **Voci sovrapposte: nessuna misura pulita.** Il proxy più vicino è il WER su AMI (Whisper large-v2
  16,82) e nessun modello aperto di questo elenco dichiara di trascrivere parlato simultaneo invece
  di sceglierne uno.
- **Recitazione urlata, sussurrata, stilizzata: zero misure**, ovunque. Il proxy più vicino è il
  degrado con rumore MUSAN dichiarato da NVIDIA per Parakeet (SNR 0 → +84% relativo, SNR -5 → +214%),
  che dice solo che sotto SNR 0 crolla tutto.
- **Canto sul coreano: nessuna evidenza esiste.** Qwen misura canto e musica di sottofondo solo su
  set inglesi e cinesi. Sul cinese l'evidenza c'è (opencpop 1,12% CER per FireRedASR2, M4Singer
  5,98 WER per Qwen) ed è materiale da studio, non un OP mixato.
- **Timestamp su audio con musica: non misurati da nessuno.** Il report Qwen dice che i metodi
  concorrenti "degradano marcatamente in contesti long-form con musica" e non pubblica nessuna
  tabella specifica su musica. È una affermazione degli autori senza numero dietro.
- **Timestamp dopo fine-tuning: quasi mai verificati.** Un fine-tune su clip corte senza token di
  timestamp è il modo documentato di perdere la predizione dei timestamp, e nessuno dei fine-tune
  coreani o cinesi esaminati pubblica una singola misura su questo. Per Sublore è la proprietà
  portante e va controllata a mano su ogni candidato.
- **Coreano su Qwen3-ASR: due letture dello stesso report non concordano.** Una lettura di
  arXiv 2601.21337 riporta FLEURS ko 2,57, CommonVoice ko 5,88, MLC-SLM ko 8,61; un secondo lettore
  dello stesso report afferma che non esistono CER coreani per dataset, solo medie multilingue. Non è
  stato possibile risolvere la discrepanza senza rileggere il PDF. Non citare quei numeri prima di
  averlo fatto.
- **Supporto Parakeet in whisper.cpp: due resoconti incompatibili.** Un ricercatore riporta di aver
  letto l'albero del repo e trovato supporto nativo dalla v1.9.0 (2026-06-17, PR #3735), con
  `examples/parakeet-cli/`, `models/convert-parakeet-to-ggml.py` e pesi ufficiali
  `ggml-org/parakeet-GGUF`; un altro descrive il runtime come il progetto terzo
  [mudler/parakeet.cpp](https://github.com/mudler/parakeet.cpp) (MIT, binari precompilati Linux e
  Windows, Vulkan e CUDA). Le due cose possono coesistere, ma la differenza cambia il costo di
  adozione da "flag di build" a "secondo sidecar". Da chiudere buildando, non leggendo.
- **Niente in questo documento è stato eseguito.** Nessun modello scaricato, nessuna conversione
  provata, nessun CER riprodotto. Non c'è nessun verdetto comportamentale qui, né su Linux né su
  Windows, e non deve essere presentato come tale.

### La cosa da costruire prima della prossima ricerca

Una fixture di Sublore, che oggi non esiste ed è l'unico artefatto di questo intero lavoro che
sarebbe specifico del prodotto. Venti-trenta minuti per lingua: dialogo pulito, dialogo sopra colonna
sonora, recitazione urlata e sussurrata, una canzone inserto che sborda nel dialogo, una scena a voci
sovrapposte, e per il coreano un set piccolo etichettato a mano che copra 하십시오체, 해요체, 해체 e
반말 per misurare il registro. Trascritta a mano con timing di battuta, tenuta nel repo come fixture
di regressione secondo CONTRIBUTING.md §5.3, e valutata con CER, sWER, un diff di sola spaziatura per il
coreano, e SubER per timing e segmentazione insieme. Con quella, "Belle contro large-v3" e
"large-v3 contro Parakeet" diventano una misura di un pomeriggio invece che una discussione.
