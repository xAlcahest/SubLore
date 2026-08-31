# Windows: `model_download` che falliva una corsa su tre, e non era la rete

Diagnosi del 2026-08-31, ramo `fix/save-writes-what-it-says`. Sequel di
`docs/reports/windows-entrypoint.md`: risolto il caricamento dei binari, `check (windows-latest)` è
diventato verde una volta e rosso le due successive **sullo stesso commit**.

## Tre sintomi che sembravano tre problemi

| corsa       | test                                                             | esito                                    |
| ----------- | ---------------------------------------------------------------- | ---------------------------------------- |
| 33375080302 | `a_model_damaged_in_place_is_fetched_again...`                   | `NetworkFailed: io: os error 10053`      |
| 33376275439 | `an_interrupted_download_resumes_from_where_it_stopped`          | il file `.part` non esiste: `NotFound`   |
| 33376277837 | `a_server_offering_a_different_file_is_refused_before_a_byte...` | `NetworkFailed` invece di `SizeMismatch` |

Undici test, ogni volta uno solo rosso, ogni volta uno diverso. È la forma di una corsa persa, non di
un difetto in tre posti.

## Due tentativi, e cosa hanno insegnato

Il primo: il server di prova drenava il socket prima di chiudere, con 200 ms di bound. Ha spostato la
frequenza senza toccare la causa.

Il secondo, sbagliato e istruttivo: mandare sempre `Connection: close`. Il ragionamento era che con
`Content-Length` e keep-alive il client non ha motivo di chiudere per primo, quindi chiude il server,
e Windows trasforma una chiusura con qualcosa ancora non letto in un reset. Coerente, e ha rotto due
test che prima passavano: con `close` ureq legge fino a EOF e alza un errore di trasporto **prima** di
consegnare i byte al chiamante, quindi il `.part` non viene mai scritto e l'header con la lunghezza
mentita non viene mai valutato. I due test avevano ragione; la modifica no, ed è stata revocata.

Vale la pena dirlo: quel tentativo è stato smascherato solo perché le corse erano tre. Una sola sarebbe
stata verde con probabilità di circa uno su tre, e la modifica sarebbe entrata come "risolto".

## La causa

`FakeServer::start` mette il listener in `set_nonblocking(true)`, così il thread può accorgersi di
`stop` invece di restare bloccato in `accept()`. Poi passa a `serve` il socket accettato.

Su Linux il socket accettato **non** eredita il flag. Su Windows **lo eredita**: Winsock dà al socket
di `accept()` le proprietà di quello in ascolto. Quindi su Windows, e solo lì, il primo
`reader.read_line(&mut line)` di `serve` può rispondere `WouldBlock` se i byte della richiesta non
sono ancora arrivati. La riga è `unwrap_or(0)`, zero significa EOF, e `serve` ritorna senza rispondere
niente.

Da lì i tre sintomi sono lo stesso evento visto da tre test diversi: connessione chiusa senza
risposta. Chi stava leggendo un corpo prende `10053`; chi si aspettava 120 byte nel `.part` non trova
il file perché non è stato scritto un byte; chi si aspettava `SizeMismatch` non ha mai letto l'header
in cui la lunghezza era mentita.

E spiega anche l'intermittenza, che è la parte che rendeva tutto confuso: dipende solo da quanto
presto il pacchetto della richiesta arriva rispetto alla prima `read`.

## Il rimedio

Una riga: `stream.set_nonblocking(false)` in cima a `serve`, con il motivo scritto sopra. Il resto
del server torna com'era — l'`unwrap_or(0)` adesso è di nuovo onesto, perché in modalità bloccante
zero significa davvero EOF.

## Come è stato verificato

Undici test cinque volte di fila in locale, clippy pulito, workspace intero senza fallimenti. Su
Windows lo dicono tre corse in CI sullo stesso commit, perché su un guasto uno-su-tre un verde solo
non è una prova ed era già successo di crederci.
