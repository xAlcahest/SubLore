# ASR per anime: cosa esiste davvero

Ricerca desk, chiusa il 2026-08-30. Nessun modello è stato scaricato, convertito o eseguito: tutto
quello che segue viene da model card, file di benchmark, config, header binari letti da remoto,
issue tracker e paper. Dove una cosa non è stata verificata eseguendola, è scritto.

## 1. La risposta

Un ASR costruito per l'anime, nel senso di audio di animazione giapponese trasmessa, con musica,
effetti, canto e voci sovrapposte, **non esiste**. Esiste una famiglia di fine-tune Whisper e
Qwen3-ASR etichettati "anime" che sono misurabilmente migliori di Whisper base sul loro dominio, ma
quel dominio non è l'anime: è la voce delle visual novel. Tutti, senza eccezioni, discendono da due
corpora, `litagin/Galgame_Speech_ASR_16kHz` (5.353 ore) e `joujiboi/japanese-anime-speech-v2`
(450 ore), ed entrambi sono tracce vocali estratte da videogiochi commerciali giapponesi: registrazioni
di studio asciutte, un attore alla volta, nessun letto musicale, nessun SFX, nessuna sovrapposizione,
nessun canto, perché il motore di gioco mixa la BGM a runtime. La card di joujiboi lo dice testualmente:
"Dataset source: **visual novels**". In questo ecosistema "anime" nomina uno **stile di recitazione**,
non una sorgente audio. Il che significa che di sei difficoltà elencate nella domanda questi modelli
ne toccano una e mezza (recitazione stilizzata, urlata, sussurrata, e vocalizzazioni non lessicali),
peggiorano attivamente sui nomi propri (i termini escono scritti con i kanji della visual novel di
addestramento, quindi un nome di personaggio può uscire come quello di un'altra opera), e sulle
altre quattro non ci sono né dati né motivo di aspettarsi un miglioramento. Sopra a questo ci sono
due blocchi indipendenti: il modello più usato della categoria, `litagin/anime-whisper`, è MIT ma
addestrato su un corpus la cui licenza vieta esplicitamente l'uso commerciale del dataset e di
qualunque modello che ne derivi, e Sublore vende i moduli pro; e i modelli che nei benchmark vanno
meglio, quelli di `efwkjn`, **non dichiarano nessuna licenza**, quindi non sono ridistribuibili. La
raccomandazione operativa è quindi: restare su `whisper-large-v3-turbo` come default, perché è MIT,
nativo in whisper.cpp, ha timestamp word-level veri e non crolla sull'audio broadcast; investire la
mezza giornata di lavoro non nel modello ma nella **segmentazione**, che è l'unica leva con una
misura pulita alle spalle; e trattare i nomi propri e la terminologia come un problema post-ASR, cioè
esattamente il termbase, che è il prodotto.

## 2. Cosa esiste

| Modello                                                                                                   | Cos'è                                                                                                                                                                                                                      | Licenza                                                                                                                                                                                                                                                                                                        | Evidenza (tipo)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     | whisper.cpp                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| --------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [litagin/anime-whisper](https://huggingface.co/litagin/anime-whisper)                                     | Fine-tune di kotoba-whisper-v2.0 (distil di large-v3, 32 enc / 2 dec, 756M) su ~5.300 h di voce galgame. Il riferimento della categoria: 69.428 download, 155 like. Ultima modifica 2024-11-24.                            | MIT sul modello. Il corpus `Galgame_Speech_ASR_16kHz` è etichettato GPL-3.0 con clausola aggiuntiva che vieta l'uso commerciale "del dataset e di qualunque modello addestrato con esso". Conflitto non risolto.                                                                                               | **Misurato** dall'autore su 5 visual novel escluse dal training (~75k clip): CER medio 13,0 contro large-v3 16,5, turbo 16,9, kotoba-v2.0 18,8, parakeet-ja 18,6, reazonspeech-nemo-v2 23,6. **Misurato da terzi** (BENCH.md di efwkjn) conferma l'ordine in-dominio, e mostra il crollo fuori dominio: TEDxJP 41,0 / 59,5 CER, jsut-book 49,0 / 62,8, con tassi di cancellazione dal 60 al 69%.                                                                                                    | Conversione ggml pubblicata due volte ([Aratako](https://huggingface.co/Aratako/anime-whisper-ggml), MIT, 2025-06-16, f16 1,52 GB fino a q2_k 269 MB; [Jaffe2718](https://huggingface.co/Jaffe2718/ggml-whisper-unofficial), 2025-12-06). Il convertitore Aratako scrive nella sua card che l'inferenza parte ma l'output è sbagliato, con "caratteri misteriosi" in testa, e chiede aiuto. Nessuno ha pubblicato una correzione né un CER dentro whisper.cpp.                                                                                                                                                                                                 |
| [efwkjn/whisper-ja-anime-v0.3](https://huggingface.co/efwkjn/whisper-ja-anime-v0.3)                       | Fine-tune completo di large-v3-turbo con tokenizer giapponese **sostituito** (vocab 20480), 4 layer decoder, 0,8B. Ultima modifica 2025-06-06.                                                                             | **Nessuna licenza dichiarata.** Nessun campo nella card, nessun tag HF, LICENSE/LICENSE.txt/LICENSE.md restituiscono 404. Di default: tutti i diritti riservati.                                                                                                                                               | **Misurato**, e il BENCH.md dell'autore è il documento migliore dello spazio perché confronta 14 modelli su 11 set con la stessa pipeline. In-dominio è alla pari con anime-whisper (14,5 / 10,8 / 18,6 / 13,3 / 9,8 sui cinque titoli); fuori dominio è molto meglio (TEDx 10,2, ReazonSpeech 9,4 contro 41,0 e 30,0 di anime-whisper).                                                                                                                                                            | **No.** Verificato leggendo l'header del binario convertito e il sorgente: `is_multilingual()` in whisper.cpp è `n_vocab >= 51865`, quindi con 20480 il fixup dei token speciali non parte e `logits[vocab.token_not]` scrive all'indice 50362 in un vector da 20480 float. Scrittura fuori dai limiti a ogni passo di decode, con il check sulla dimensione del vocab commentato nel loader, quindi il modello carica e poi corrompe memoria in silenzio. Issue [#3392](https://github.com/ggml-org/whisper.cpp/issues/3392) aperta dal 2025-08-25, la PR di fix [#3555](https://github.com/ggml-org/whisper.cpp/pull/3555) chiusa senza merge il 2026-01-21. |
| [efwkjn/whisper-ja-1.5B](https://huggingface.co/efwkjn/whisper-ja-1.5B)                                   | Fine-tune di large-v3, 1.543.490.560 parametri, 4,9 GB, forma e tokenizer standard (vocab 51866, 32/32 layer), con `alignment_heads` nel generation_config. Pubblicato e modificato il 2026-02-13.                         | **Nessuna licenza dichiarata**, stesso problema di v0.3. Non ridistribuibile.                                                                                                                                                                                                                                  | **Misurato** dal BENCH.md dell'autore, che va letto per intero e non dalla card. Sul dominio galgame vince 1 titolo su 5 nel suo file e 0 su 5 nel benchmark più recente del 760M. Dove è davvero forte è il long-form: book-batch 14,4 CER contro 18,0 del secondo, fleurs 4,3, jsut-basic 6,1. Costo: nella colonna tempi è 1,00 di riferimento contro 0,22 di turbo, cioè circa 4,5 volte più lento.                                                                                             | Sì per forma (tokenizer standard, conversione con `convert-h5-to-ggml.py` senza chirurgia sul vocab), ma nessuno l'ha eseguito. Irrilevante finché non c'è una licenza.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| [jaykwok/Qwen3-ASR-1.7B-JA-Anime-Galgame](https://huggingface.co/jaykwok/Qwen3-ASR-1.7B-JA-Anime-Galgame) | **Non è Whisper.** SFT completo di Qwen3-ASR-1.7B (encoder audio + decoder LLM Qwen3) sullo stesso corpus galgame. 2026-05-31. Quantizzazioni GGUF di terzi per [CrispASR](https://github.com/CrispStrobe/CrispASR).       | Dichiarata `other`. La card rimanda esplicitamente alla licenza del dataset prima di "redistribution, commercial use, or further fine-tuning", e quel dataset vieta l'uso commerciale. La conversione `-hf` ristampa `apache-2.0` in frontmatter, ma è un'etichetta ereditata per errore, non una concessione. | **Misurato** dall'autore su un set fisso di 800 clip (seed dichiarato, CER stretto): 0,1285 contro 0,1437 del base, cioè 10,6% relativo, guadagnato quasi tutto tagliando le cancellazioni (0,0418 verso 0,0242). Anime Speech 0,0799, Nekopara 0,2276. JSUT e Common Voice peggiorano. L'autore lo chiama sanity check, non risultato da classifica.                                                                                                                                               | **No.** Il supporto Qwen3-ASR vive in llama.cpp (path mtmd) o in CrispASR. whisper.cpp non ha supporto e l'unica issue Qwen aperta è la [#1710](https://github.com/ggml-org/whisper.cpp/issues/1710) del 2024-01-01. Adottarlo significa un secondo sidecar, un secondo formato, una seconda storia Vulkan/CPU, e la issue llama.cpp [#26749](https://github.com/ggml-org/llama.cpp/issues/26749) ancora aperta all'2026-08-08 sull'`asr_text` che i client non sanno gestire.                                                                                                                                                                                 |
| [Qwen/Qwen3-ASR-1.7B](https://huggingface.co/Qwen/Qwen3-ASR-1.7B) (base)                                  | ASR multilingue generale, non anime. Citato perché è l'unico con una misura su audio mediatico giapponese reale invece che su clip di gioco. 2026-01-28. Esiste `Qwen3-ForcedAligner-0.6B` per timestamp carattere/parola. | Apache-2.0, pulita.                                                                                                                                                                                                                                                                                            | **Misurato da terzi ma debole**: benchmark Neosophie su 20 clip / ~580 s di media giapponese reale (news, varietà, drama/anime, sovrapposizioni), primo su 10 con WER 0,185 / CER 0,140 contro 0,218 / 0,184 di turbo. Venti clip sono un aneddoto con la virgola, non un benchmark. Il technical report (arXiv 2601.21337) dà CER giapponese su FLEURS 5,20, CommonVoice 11,64, MLC-SLM 11,80, tutti letti o conversazionali. La robustezza su canto e musica è misurata solo in inglese e cinese. | No, stessa storia del punto sopra.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| [openai/whisper-large-v3-turbo](https://huggingface.co/openai/whisper-large-v3-turbo) e large-v3          | La baseline. Nessun checkpoint OpenAI più recente trovato al 2026-08-30.                                                                                                                                                   | MIT.                                                                                                                                                                                                                                                                                                           | **Misurato**, ed è il punto scomodo: su audio in stile anime large-v3 sta a 16,5 CER e batte kotoba-whisper-v2.0 (18,8), parakeet-ja (18,6) e reazonspeech-nemo-v2 (23,6). Su audio broadcast ReazonSpeech sta a 14,9 e non crolla come i modelli specializzati.                                                                                                                                                                                                                                    | Nativo, zero conversione, timestamp word-level via DTW con preset di alignment heads già in `whisper.h`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| [nvidia/parakeet-tdt_ctc-0.6b-ja](https://huggingface.co/nvidia/parakeet-tdt_ctc-0.6b-ja)                 | FastConformer TDT+CTC ibrido, 0,6B, addestrato su ReazonSpeech v2.0. Creato 2024-05-13, ultima modifica 2025-02-18. **Non è un modello anime**, è qui come evidenza negativa.                                              | CC-BY-4.0, compatibile GPL-3.0 con attribuzione.                                                                                                                                                                                                                                                               | **Misurato**: ottimo sul letto (JSUT 6,4 CER, TEDxJP 9,0, tutti set di parlato letto o preparato, con punteggiatura e caratteri non alfabetici rimossi prima dello score, cioè proprio dove vive il contenuto non lessicale). Su anime: 18,6 CER, sesto su nove, dietro a **tutta** la famiglia Whisper compreso large del 2022. Chi è forte sul parlato letto giapponese non trasferisce sull'anime.                                                                                               | Parziale e non verificato. whisper.cpp ha un backend Parakeet nativo dalla v1.9.0 (2026-06-17, PR #3735) con `parakeet-cli` e timestamp per token, ma il loader implementa solo l'arch TDT: questo checkpoint è ibrido e le tensor `ctc_decoder.*` verrebbero scritte dal convertitore e poi rifiutate. Patch piccola, mai fatta. Nessun ggml giapponese pubblicato.                                                                                                                                                                                                                                                                                           |
| [kotoba-tech/kotoba-whisper-v2.0](https://huggingface.co/kotoba-tech/kotoba-whisper-v2.0)                 | Distil di large-v3 su ReazonSpeech, il modello giapponese più noto. Base di anime-whisper. Org ferma dal 2024-10-23.                                                                                                       | Apache-2.0, e [ggml ufficiale](https://huggingface.co/kotoba-tech/kotoba-whisper-v2.0-ggml) pubblicato.                                                                                                                                                                                                        | **Misurato**: sull'anime perde contro large-v3 su tutti e cinque i set (18,8 contro 16,5). La sua card non ha mai rivendicato niente sull'anime, la cornice anime è di chi lo cita. Utile solo come controllo in un A/B.                                                                                                                                                                                                                                                                            | Sì, nativo, nessuna conversione.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |

Due dataset, perché sono la causa di tutto quanto sopra:

- [litagin/Galgame_Speech_ASR_16kHz](https://huggingface.co/datasets/litagin/Galgame_Speech_ASR_16kHz), 3.746.131 coppie, 5.353,9 ore, ultima modifica 2024-10-14. Frontmatter `license: gpl-3.0`, ma il `license_link` punta a un LICENSE.md che restituisce 404, e la card riporta testualmente: "Commercial use is prohibited. This dataset and any model trained using this dataset cannot be used for any commercial purposes... Models trained using this dataset must be open-sourced." Il repo a monte, OOPPEENN/Galgame_Dataset, oggi è irraggiungibile al path citato e vive sotto un nome esadecimale con accesso ristretto. L'audio è estratto da visual novel commerciali, quindi chi ha caricato non ha i diritti per concedere né GPL né altro. Testo di licenza letto verbatim dal raw README, questa è fonte primaria.
- [joujiboi/japanese-anime-speech-v2](https://huggingface.co/datasets/joujiboi/japanese-anime-speech-v2), 292.637 clip, 397,54 h SFW + 52,36 h NSFW, clip media 5,3 s, contenuto fermo a luglio 2024. Tag `license: gpl` senza versione e senza file LICENSE, mentre la prosa della card dice il contrario ("openly available for commercial or non-commercial use"). Fonte dichiarata: visual novel. Bias documentati dall'autore stesso: sbilanciamento verso voci femminili, vocabolario su amore e fantasy, e "the professionally produced nature of the audio results in clear and slow speech".

## 3. Cosa sposta l'ago più del modello

**La segmentazione, e non è vicino.** L'unico esperimento pulito che isola le due variabili è
[arXiv 2506.15514](https://arxiv.org/abs/2506.15514) (Whisper large-v2, Jam-ALT): separare la voce
dalla musica porta il WER long-form da 23,02 a 22,87, cioè niente; sostituire la segmentazione nativa
di Whisper con confini basati su VAD lo porta da 23,02 a 20,35. Gli autori lo dicono esplicitamente:
il guadagno viene dai confini dei segmenti, non dalla qualità vocale dopo la separazione. Per un
editor di sottotitoli, dove il confine di battuta **è** il prodotto, questa è la leva giusta ed è già
dentro il runtime — **ma questa parte è superata, vedi la nota sotto**: whisper.cpp ha il VAD Silero nativo (`--vad` con `ggml-silero-v6.2.0.bin`) e
espone `--vad-threshold`, `--vad-min-silence-duration-ms`, `--vad-speech-pad-ms`,
`--vad-max-speech-duration-s`, `--vad-samples-overlap`. Costa zero dipendenze nuove.

> **Superato, 2026-08-30, decisione 16.** Il proprietario ha escluso il Silero interno per questo
> dominio e ha spostato la taratura dei confini su un VAD esterno con rimappatura dei timestamp,
> parcheggiato come task di pipeline post-v1 e non come flag. La misura qui sopra non cambia — è la
> segmentazione a spostare il numero, non la separazione — ma lo strumento sì, e presentare il flag
> interno come leva gratuita non è più la raccomandazione. La fonte citata a sostegno, trascrizioni
> vuote documentate sulla card di kotoba-whisper-v2.2, non è stata trovata né su quella card né su
> quella di v2.1: vedi `decisions.md` 16.

**La separazione sorgente, da sola, è più rischiosa che utile.** Tre studi misurati vanno nella
stessa direzione. In [arXiv 2603.04710](https://arxiv.org/abs/2603.04710) separare prima di Whisper
porta il WER inglese da 10,53 a 21,66 e il bengalese da 65,83 a 77,35, mentre il PSNR _migliora_: la
qualità percettiva e l'accuratezza ASR si scollano, perché gli artefatti di separazione sono fuori
distribuzione per Whisper. In [arXiv 2512.17562](https://arxiv.org/abs/2512.17562) il denoising
peggiora tutte e 40 le configurazioni testate. Nel paper Jam-ALT sopra, la separazione fa salire il
proxy di allucinazione da 0,05 a 1,29. L'unico caso in cui aiuta è su MUSDB-ALT, dove il separatore
era stato addestrato sul test set, e sono gli autori a segnalarlo. Il compromesso sensato è quello di
[WhisperJAV](https://github.com/meizhong986/WhisperJAV) (MIT, v1.9.0 del 2026-08-15), che ha
letteralmente una checkbox "Enhance for VAD only": lo stem pulito guida il rilevamento del parlato,
il modello sente l'audio originale. Prendi il guadagno sui confini, che è misurato, senza il rischio
artefatti, che pure è misurato. L'anime con BGM continua e sigle sopra il dialogo è il caso in cui la
separazione dello stem potrebbe comunque valere, ma nessuno ha pubblicato un numero: è un'ipotesi, non
un piano.

**Il prompting non è una leva di terminologia, e questo tocca il cuore del prodotto.** Gli `hotwords`
di faster-whisper non sono un algoritmo di biasing: leggendo `get_prompt()` in `transcribe.py`, la
stringa viene tokenizzata, preceduta da `sot_prev` e troncata a `max_length//2`, e finisce nello
stesso slot del decoder di `initial_prompt` e del testo del segmento precedente. Stesso meccanismo,
nome più bello. Quindi whisper.cpp con `--prompt` dà già tutto quello che darebbe faster-whisper, e
tutte le misure negative sul prompting valgono anche lì:
[arXiv 2406.05806](https://arxiv.org/abs/2406.05806) trova che il miglioramento non è garantito
neppure quando il modello aderisce dimostrabilmente al topic del prompt; la card di anime-whisper
avverte in modo esplicito che un initial prompt causa allucinazioni e degrado grave; e nel BENCH.md
di efwkjn il condizionamento sul testo precedente porta large-v3 da 9,2 a 65,9 CER su TEDx con tasso
di inserzione 36,5. whisper.cpp non ha nessuna API hotword (issue
[#1979](https://github.com/ggml-org/whisper.cpp/issues/1979) aperta dal 2024-03-20) e non ha
`no_repeat_ngram_size`, che nella stessa tabella è il singolo rimedio più efficace ai loop di
ripetizione. **Conclusione operativa: i nomi dei personaggi e i termini inventati non si risolvono in
fase di ASR, si risolvono in una passata di correzione e QA sul trascritto.** Che è esattamente il
termbase di Sublore. Quello che sembrava un buco è in realtà un argomento a favore del prodotto.

**Quello che la comunità fa davvero non è trascrivere, è allineare.**
[SubPlz](https://github.com/kanjieater/SubPlz) (MIT, faster-whisper + stable-ts + Silero VAD + Alass)
esiste soprattutto per sincronizzare un testo già esistente sull'audio, non per generarlo da zero, e
produce più varianti di algoritmo per episodio lasciando scegliere all'utente, che è un'ammissione
onesta che nessun metodo singolo è affidabile. Chi sottotitola anime in genere ha già del testo: uno
script ufficiale, un sub JP raw, la terminologia della stagione precedente. L'allineamento forzato di
testo noto è un problema molto meglio posto della trascrizione aperta di dialogo urlato e sepolto
nella musica, ed è dove l'accuratezza di cue è realmente raggiungibile. Per un prodotto di memoria di
traduzione questo è un incastro diretto.

## 4. Cosa Sublore può spedire, e quanto costa

CLAUDE.md dice che la trascrizione è una commodity che avvolgiamo e che il prodotto è la memoria
terminologica. Questa ricerca lo conferma dall'esterno: non c'è nessun modello anime che valga
abbastanza da spostare il baricentro del progetto sull'ASR, e una raccomandazione che lo facesse
sarebbe la raccomandazione sbagliata. Nell'ordine:

1. **Default: `whisper-large-v3-turbo`, e non cambiare.** MIT, nativo in whisper.cpp, timestamp
   word-level con preset DTW già presenti, non crolla sull'audio broadcast. Costo: zero, è già la
   scelta. Da non fare: nessun modello specializzato come default.
2. **Un pomeriggio sulla segmentazione VAD.** Attivare `--vad` con Silero e tarare
   `min-silence-duration`, `speech-pad` e `max-speech-duration` contro fixture di episodio reali,
   misurando l'errore sui confini di cue e non solo il CER. È l'unico intervento con un numero pulito
   alle spalle (2,7 punti di WER nel paper Jam-ALT contro 0,15 della separazione), non aggiunge
   dipendenze, e produce criteri di accettazione osservabili come li vuole la sezione 5 di CLAUDE.md.
   Costo: ore, su Linux, dentro il sidecar esistente.
3. **La terminologia sta nel termbase, non nel prompt.** Non costruire nessuna via che passi
   `initial_prompt` o hotwords per forzare i nomi dei personaggi: la letteratura dice che non funziona
   in modo affidabile e la card del modello più specializzato dice che degrada. La correzione dei nomi
   propri, delle onorifiche e dei termini inventati è una passata post-ASR sul trascritto, cioè il
   modulo pro che già stiamo costruendo. Costo: nessuno aggiuntivo, è il prodotto.
4. **Esperimento a basso costo, se e solo se c'è curiosità:** buildare whisper.cpp corrente, prendere
   il q5_0 di `Aratako/anime-whisper-ggml` (538 MB) e il q5_0 di `kotoba-whisper-v2.0-ggml` come
   controllo, e passarci sopra un episodio vero su Linux. Le due domande sono, in quest'ordine: il
   difetto dei caratteri spuri in testa segnalato dal convertitore si riproduce? E i confini di cue
   cadono dove cade il dialogo in una scena con musica? Se la prima risposta è sì, l'idea muore lì e
   sono state spese due ore. Costo: due ore e mezzo giga di disco. Da fare **fuori** dal percorso v1.
5. **Cosa non spedire, e perché.** `litagin/anime-whisper` non va incluso né scaricato
   automaticamente dall'app finché la questione licenza non è una decisione presa dal proprietario:
   il modello è MIT ma il corpus vieta l'uso commerciale del dataset _e dei modelli che ne derivano_,
   e la clausola vieta l'**uso**, non solo la ridistribuzione, quindi non si aggira né cambiando la
   licenza di Sublore né caricandolo dal lato chiuso. La postura sopravvivibile, se lo si vuole
   comunque, è che Sublore non spedisca, non specchi e non scarichi mai quei pesi, e che l'utente
   punti l'app a un file che si è procurato da sé. Meno esposizione e meno qualità di prodotto, quindi
   è una scelta da fare deliberatamente. I modelli `efwkjn` sono fuori discussione finché non
   dichiarano una licenza. La famiglia Qwen3-ASR è fuori architettura: secondo sidecar, secondo
   formato, seconda storia Vulkan/CPU, e tutto da far compilare e girare anche su Windows, per un
   guadagno misurato su 200 clip di anime. Sono costi reali, non dettagli.

Una nota di prior art da tenere, non da adottare: la conversione `-hf` del fine-tune jaykwok
distribuisce due teste CTC di allineamento addestrate sull'encoder congelato, che danno timestamp a
livello di carattere con risoluzione 38,5 ms e rilevamento dei blank per i confini di chunk. È la
cosa più mirata alla granularità di cue che abbia trovato in tutto lo spazio. Le teste sono però
specifiche di quell'encoder, si caricano fuori dal path GGUF e ereditano lo stesso problema di
licenza. Vale come idea da reimplementare su un encoder con licenza pulita, non come modello da
prendere.

## 5. Dove finisce la risposta

Elenco esplicito di quello che nessuno ha misurato, perché è dove il proprietario deve sapere che
stiamo andando a naso:

- **Nessun benchmark ASR esiste su audio anime broadcast reale.** Zero. Ogni numero "anime" citato
  qui è su tracce vocali di visual novel, cioè studio, un parlante, asciutto. Non esiste una misura
  di parlato sopra un letto musicale denso, di sigle che sbordano sul dialogo, di canto diegetico, né
  di voci sovrapposte, per nessuno di questi modelli.
- **Nessuna misura di accuratezza dei timestamp a livello di cue, in giapponese, per nessun modello**,
  a parte i 42,4 ms di Accumulated Average Shift dichiarati per Qwen3-ForcedAligner su dati etichettati
  MFA. Per un editor di sottotitoli questo buco pesa più di qualunque differenza di CER.
  `anime-whisper` in particolare non emette timestamp usabili: c'è una discussion aperta su HF dal
  2025-05-10 in cui la pipeline non restituisce timestamp neanche con `return_timestamps=True`, e chi
  lo usa in produzione (WhisperJAV) prende i tempi dal VAD e non dal modello.
- **Nessuna misura su nomi propri, onorifiche e termini inventati.** L'unica cosa documentata è
  negativa e viene dall'autore stesso: i nomi propri escono con i kanji della visual novel di
  addestramento.
- **Non ho eseguito nulla.** Nessun peso scaricato, nessuna conversione ggml lanciata, nessun secondo
  di audio trascritto, né su Linux né altrove. In particolare restano non verificati: che le
  conversioni ggml di anime-whisper producano giapponese usabile (il convertitore stesso dice di no),
  che i timestamp di un decoder distil a 2 layer siano abbastanza buoni per i sottotitoli, che il
  Parakeet giapponese si converta con la patch minima descritta.
- **Confronti tra tabelle diverse non sono rigorosi.** I numeri di litagin e quelli di efwkjn vengono
  da harness diversi, con impostazioni di decoding diverse, e i test set di litagin sono privati e non
  riproducibili. Sono indicativi, non da citare come misure comparabili.
- **Esiste un corpus di anime vero e nessuno ci ha mai valutato un ASR.** Anim-400K
  ([arXiv 2401.05314](https://arxiv.org/abs/2401.05314), UC Berkeley, 2024-01-10) è composto da oltre
  425.000 clip allineate, 763 ore, 190+ opere, tracce giapponese e inglese con sottotitoli inglesi
  sulla traccia giapponese. Se qualcuno volesse una risposta difendibile alla domanda "quale modello è
  il migliore sull'anime", quello più le nostre fixture SRT sono la strada, e sarebbe il primo numero
  pubblico del genere. L'unico paper che misura davvero il parlato mescolato a musica è Woo, Mimura,
  Yoshii e Kawahara, [arXiv 2008.12048](https://arxiv.org/abs/2008.12048) del 2020, pre-Whisper: hanno
  costruito un set mescolando parlato giapponese con ~30 ore di musica da animazione giapponese e
  trovato che la separazione nel dominio del tempo ottimizzata congiuntamente con il backend ASR
  migliora molto il risultato. Sei anni, e nessun seguito moderno sull'anime.
