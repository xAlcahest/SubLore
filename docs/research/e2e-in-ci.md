# L'end-to-end su CI: è possibile, o è condannato?

Ricerca commissionata dal proprietario, 2026-08-31. Tutto quello che segue riguarda Linux. Su
Windows non è stato verificato nulla, e §3.11 spiega perché quella parte è messa peggio di questa.

---

## 1. La risposta

**Non è condannata, e per la maggior parte non sta nemmeno fallendo.** Nell'ultima run su main
(33360311284, 2026-08-31) cinque check su sette sono verdi su ubuntu-latest: smoke (8 spec file, 33
test, inclusi doppio click sulla lista cue e digitazione reale via XTEST), shutdown 5/5,
startup-args 7/7, mpv-context 5/5, scale 5/5. Il verdetto stampa esattamente
`failing checks: close-gate late-edit`. E i due rossi non sono nessuno dei due problemi che ci
aspettavamo: close-gate supera ogni passo a livello X (il dialogo è mappato, discard esce con
status 0, nessun processo sopravvive) e poi fallisce perché **il save scrive un file identico
byte per byte**, che è una domanda su prodotto o modello, non sull'input; late-edit muore su
`X Error: 9: Bad Drawable` perché la harness ha tenuto un window id attraverso la distruzione della
finestra, che è una race TOCTOU nel suo stesso codice di ispezione. Detto questo, la forma è rara
al limite dell'inesistente: nel campione, **cinque repository** tengono verde una suite
tauri-driver su GitHub Actions con interazione reale, **uno solo** (gitbutlerapp/gitbutler) è
un'app di terze parti che gatea ogni PR sulla propria UI reale, e la sua suite intera è un file
spec con un test; **zero** repository al mondo guidano una finestra video libmpv sotto Xvfb; **zero**
usano tauri-driver più xdotool come percorso di input primario su un gate PR. E la premessa su cui
è costruita tutta la scelta di design, cioè che WebKitWebDriver rifiuti Element Click e Actions
contro una webview wry, **è un artefatto di packaging Fedora e non vale sul runner**: è vera su
questa macchina, falsa su ubuntu-latest. Il problema più urgente però non è nessuno di questi: **il
passo Verdict può dichiarare verde una run rossa**, ed è un bug che ho riprodotto (§3.2).

---

## 2. Cosa fanno davvero i progetti campionati

Conteggi, non aggettivi. Tutto letto il 2026-08-31 salvo dove indicato.

### 2.1 Quanti tengono verde un E2E Tauri interattivo

Ricerca codice su `.github/workflows`: `tauri-driver` compare in 29 repository distinti (40 hit
`--extension yml`, 7 `--extension yaml`). Di questi, cinque tengono verde una lane con interazione
reale:

| Repository                   | Cosa gira                                                                                                  | Stato misurato                                             | Cosa prova davvero                                                                                                                                                        |
| ---------------------------- | ---------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `webdriverio/desktop-mobile` | `_ci-e2e-tauri-all-providers.reusable.yml`, `xvfb-run --auto-servernum`, nessun WM                         | 24 job su 24 verdi in 5 run recenti; run 32796847588 verde | `e2e/test/tauri/actions.spec.ts` esercita **sia elementClick sia performActions**. Su Linux il provider tauri-driver è **required**, l'allow-fail è solo Windows          |
| `gitbutlerapp/gitbutler`     | `test-e2e.yml`, `xvfb-run pnpm test:e2e:blackbox` in container pinnato a digest, nessun WM, nessun xdotool | 25 job su 25 verdi nel campione di 60 run                  | `e2e/blackbox/tests/add-project.spec.ts`: **un solo test**. `element.click()` nativo. La copertura vera sta in una suite Playwright separata su build web, shardata per 4 |
| `gptme/gptme`                | `tauri.yml`, tauri-driver 2.0.6 sotto xvfb-run                                                             | 10 run su 10 verdi, ~3 min                                 | Il workflow **scrive un `webui/dist/index.html` mock** e guida quello, non il frontend vero                                                                               |
| `pubkey/rxdb`                | `examples/tauri`, 2 test                                                                                   | verde su master                                            | app di esempio                                                                                                                                                            |
| `shm11C3/HardwareVisualizer` | smoke first-run via selenium-webdriver                                                                     | 6 su 6 verdi                                               | la copertura interattiva vera è Playwright su harness mock                                                                                                                |

Il rapporto onesto: di venti repository Tauri applicativi fra i più stellati, **uno** ha un workflow
E2E. `libnyanpasu/clash-nyanpasu` (13k stelle) installa perfino `webkit2gtk-driver` e `xvfb` e porta
il commento tauri-driver copiato dai doc, ma i suoi job sono lint, build e test_unit e basta.
Sublore non è indietro rispetto al campo: il campo è quasi vuoto.

### 2.2 Il controcampione: come muore una suite del genere

`mediar-ai/screenpipe`, `e2e-test.yml`, job "Linux E2E Tests (advisory)": su 100 run,
**0 successi, 64 fallimenti, 11 cancellati**. Nessuno se ne accorge perché il job ha
`continue-on-error: true` e la parola "(advisory)" nel nome. Nessuna assertion è stata indebolita,
nessun test saltato, e nessuno ha dovuto sistemarlo. È esattamente il fallimento che le regole di
Sublore vietano, ottenuto senza violare nessuna di esse alla lettera.

Lo stesso repository mostra anche la metà onesta: una lane piccola e veloce (un singolo test di
integrazione Rust sotto Sway headless, timeout 30 minuti) è required e chiusa da un job rollup che
verifica che sia davvero passata.

### 2.3 Chi fa la forma di Sublore

`martinkoutecky/tine` è l'unico repository al mondo che combina tauri-driver, WebKitWebDriver,
WebdriverIO, xdotool e Xvfb, cioè la forma di Sublore, più openbox. Fatti:

- `.github/workflows/ui-e2e.yml` gira su `workflow_dispatch` soltanto. Non gatea niente.
- Il job `Linux x64 real-app release suite` **non ha mai eseguito**: skipped in tutte le 25 run più
  recenti fino al 2026-08-10, un cancelled il 2026-08-27, zero success e zero failure.
- Su ~60 scenari Linux, **4** impostano `E2E_WINDOW_MANAGER: openbox`. Gli altri 56 girano senza WM.
- I 4 con openbox hanno bisogno anche di `E2E_ALLOW_SYNTHETIC_FOCUS=1` e di un retry
  infrastrutturale, perché (parole loro) openbox sotto l'Xvfb annidato di GitHub lascia
  `_NET_ACTIVE_WINDOW` puntato a un frame appena distrutto.

Quindi: "aggiungi un window manager e sarai in una configurazione con precedente verde" **non ha
precedente verde**. L'unico progetto che lo fa non ha mai avuto una run Linux verde su CI e
documenta che openbox introduce una classe di flake propria.

### 2.4 Chi fa XTEST reale, senza WM, e resta verde

`vercel-labs/native` (7602 stelle). `.github/scripts/linux-canvas-smoke.sh` installa solo
`libgtk-4-dev xvfb xdotool x11-utils`, nessun window manager, e guida un right-click reale dentro un
menu contestuale GTK4 override-redirect. **50 successi, 1 fallimento, 1 cancellato su 52 run.**
Trova il popup diffando `xwininfo -root -children` prima e dopo il click e richiedendo
`Map State: IsViewable` più dimensioni > 10px. Possiede il proprio Xvfb via `Xvfb -displayfd 4`
invece di avvolgere l'app in `xvfb-run`, proprio perché xdotool condivida il display dell'app.

Nota importante, perché la ricerca iniziale l'aveva presa al contrario: quel progetto **usa la
tastiera e chiede il focus**. `linux-canvas-smoke.sh:225` fa
`xdotool windowactivate "$win" || xdotool windowfocus "$win"`, e la versione Wine digita
`xdotool type --delay 120`. Non è "solo puntatore". Quello che il suo commento dice davvero è più
preciso e più utile: senza WM la finestra X del **popover** non riceve mai focus da tastiera, mentre
gli eventi puntatore si risolvono per posizione. Il problema del no-WM morde i popup
override-redirect, non il toplevel.

Non è Tauri (è Zig più GTK4), quindi non è precedente per lo stack. È precedente per l'input.

### 2.5 Chi guida una finestra video sotto Xvfb

Nessuno. Ricerche a zero risultati: `mpv xdotool path:.github/workflows`,
`libmpv xvfb path:.github/workflows`, `mpv --wid xvfb`. Controllati uno per uno:
`mpv-player/mpv` (solo `meson test`), `celluloid-player/celluloid` (solo `ninja test`),
`jellyfin/jellyfin-media-player` (solo `ctest`), `smplayer-dev/smplayer` (12 workflow, tutti
`build-*`, nessun test), `KDE/haruna` (nessuna directory `.github/workflows`). I due repository
Tauri+mpv esistenti (`eggprez/FellyJin`, `chrisJuresh/films`) hanno solo workflow di release.

L'adiacente più vicino, `getmydia/mydia`, gira un "Player E2E" con mpv in Docker ed è rosso su
master su tutte e quattro le run recenti.

Quindi la superficie video è territorio inesplorato. Va detto subito però: i check
`mpv-context` e `scale` di Sublore, che toccano quella superficie, **sono verdi**. Inesplorato non
vuol dire rotto.

### 2.6 Chi usa AT-SPI su GitHub Actions

`aws-deadline/deadline-cloud`, `.github/workflows/ui_tests.yml`, ubuntu-latest: installa
`xvfb dbus dbus-x11 at-spi2-core`, poi lancia a mano `at-spi-bus-launcher --launch-immediately` e
`at-spi2-registryd` dentro `dbus-run-session`. **12 run su 12 verdi**, l'ultima il 2026-08-28.
Stesso blocco in altri due loro workflow.

Campione contato: `at-spi2-registryd` in yml dà 2 hit, entrambi in quell'organizzazione;
`at-spi-bus-launcher` dà 5 hit su 3 repository (aws-deadline, `trycua/cua`,
`openinterpreter/interpreter-cua`). È un pattern funzionante, verde, mantenuto, praticato da tre
repository. Non è una convenzione di settore e non va presentata come tale. E aws-deadline guida
Qt/PySide6, quindi prova l'infrastruttura del bus su ubuntu-latest, non l'albero GTK+WebKit.

---

## 3. Cosa c'è di sbagliato nella forma di Sublore

### 3.1 La premessa dell'input è un fatto di packaging Fedora, non una proprietà di wry

`e2e/README.md:144` e `e2e/lib/input.js:6` dicono che WebKitWebDriver risponde
`unsupported operation` a Element Click, Element Send Keys e Actions contro una webview wry.

**Su questa macchina è vero.** Sonda diretta contro il vero `target/debug/sublore` sotto Xvfb, senza
WM, con tauri-driver 2.0.6 e `/usr/bin/WebKitWebDriver` di serie, HTTP grezzo sugli endpoint W3C,
UI completamente caricata:

```
FIND button:          200
ELEMENT CLICK:        500  {"error":"unsupported operation"}
ELEMENT SEND KEYS:    500  {"error":"unsupported operation"}
PERFORM ACTIONS(key): 500  {"error":"unsupported operation"}
GET title/source:     200
```

**Su ubuntu-latest è falso.** Causa, dal commento di jwatt su `tauri-apps/tauri#6541` datato
2026-06-26 (non è materiale del 2023, l'issue è aperta dal 2023 ma questo commento ha due mesi): le
distribuzioni compilano webkit due volte, una GTK3 (`webkit2gtk-4.1`) e una GTK4 (`webkitgtk-6.0`),
ma installano **un solo** binario `WebKitWebDriver`, quindi attivano `-DENABLE_WEBDRIVER=ON` su una
build sola. Solo quella build riceve anche `ENABLE_WEBDRIVER_MOUSE_INTERACTIONS` e
`_KEYBOARD_INTERACTIONS`. wry si lega a GTK3. Fedora attiva WebDriver sulla build 6.0, quindi la
libreria che wry usa non ha le interazioni, `performMouseInteraction` ritorna `NotImplemented`, e il
layer WebDriver lo traduce in `unsupported operation`.

Verificato su entrambi i lati:

- Fedora 44, questa macchina: `rpm -qf $(which WebKitWebDriver)` restituisce
  **`webkitgtk6.0-2.52.5-1.fc44`**, la build GTK4, mentre `webkit2gtk4.1-2.52.5-1.fc44` è
  installato a parte e non contiene nessun binario driver.
- Ubuntu noble, in container: `webkit2gtk-driver 2.52.3-0ubuntu0.24.04.1` dichiara
  **`Depends: libwebkit2gtk-4.1-0`**, cioè la stessa libreria GTK3 che usa wry. Stessa 2.52.x
  upstream, scelta di packaging opposta.
- Red Hat bug 2493782, aperto il 2026-06-26, **CLOSED UPSTREAM il 2026-08-21**. Quindi anche la
  parete locale è temporanea.
- Prova positiva sul runner: `webdriverio/desktop-mobile` run 32802575730, job 97796591869, log
  riga 51176, blocco provider `official`, tag `[wry 0.55.1 linux]`:
  `click({}) lands on the same target as bare click()` PASS,
  `click({ button: "left" }) lands on the target` PASS. Non è uno shim: l'override in
  `service.ts:633` è `installMockSyncOverride`, che risincronizza i mock dopo il comando vero, non
  ridirige il click su `browser.execute`.

**Conseguenza per la documentazione.** `e2e/README.md:144` afferma un limite universale che è
locale a una distribuzione. CLAUDE.md §9 impone che ogni verdetto comportamentale porti la sua
piattaforma; questa riga la porta al contrario. Va riscritta come "su Fedora 44, dove il driver
installato viene da webkitgtk6.0 e wry usa webkit2gtk4.1", con il riferimento a #6541 e al bug
Red Hat.

**Bug secondario nella stessa pagina.** `e2e/README.md:107` dice di installare `webkit2gtk4.1` per
avere `WebKitWebDriver` su Fedora. Su Fedora 44 quel pacchetto non contiene alcun binario driver:
lo contiene `webkitgtk6.0`. L'istruzione è sbagliata così com'è scritta.

**Attenzione a non rovesciare la conclusione.** Element Send Keys non è provato verde da nessuna
parte: la suite tauri di webdriverio non digita mai (`grep setValue|addValue|keys(` su
`e2e/test/tauri/` dà zero hit), gitbutler aggira con `browser.execute` che scrive `input.value`. E
`tauri-apps/tauri#15871` (aperta 2026-08-13) dice che Send Keys attraverso tauri-driver **è già
rotto** su WebKitWebDriver 2.52.3, perché tauri-driver inoltra solo l'array JSONWP `value` e non il
campo W3C `text`. Regge oggi solo perché ubuntu-latest è ancora 24.04 con un driver più vecchio e
tollerante. Quando ubuntu-latest passa a 26.04, chiunque si appoggi a `setValue` diventa rosso in
blocco.

### 3.2 Il passo Verdict non sa dire se un check ha girato

Questo è il problema più grave del documento, non è stato riportato da nessuno, ed è quello con la
correzione più economica.

`ci.yml`, passo "Verdict (red if any check failed)":

```sh
grep -qE "check passed \(|Spec Files:.* 0 failed|[0-9]+ passed, [0-9]+ total" "$log" || failed=...
```

**Correzione del 2026-08-31, dopo la prima stesura.** La versione originale di questo paragrafo
sosteneva che la terza alternativa dichiara verde una run wdio rossa, e lo mostrava con
`Spec Files: 1 failed, 7 passed, 8 total`. Quella riga è sintetica: wdio stampa `passed` prima di
`failed`, cioè `7 passed, 1 failed, 8 total`, dove `passed, ` è seguito da `1 failed` e non da
`8 total`, quindi la regex non combacia. Il 2026-08-31 uno spec si è rotto davvero in CI (run 33378863875) e il verdetto è andato rosso come doveva. **Il difetto affermato qui non era
raggiungibile, ed è stato affermato senza provarlo contro un log vero.**

Quello che resta, riprodotto contro i quattro set di log reali scaricati dagli artefatti:

- Un log che non è né un riassunto wdio né un check contato ma contiene `3 passed, 5 total`, per
  esempio l'output di un altro strumento, passa. Reale ma marginale.
- **Un check che non ha mai girato non lascia nessun log, e il ciclo non ha niente su cui fallire.**
  Se il passo close-gate non parte, il verdetto guarda sei log verdi e dice verde. Questo è il buco
  serio, e nessuna versione basata sul testo lo chiude.
- Una run wdio che non ha eseguito nessuno spec stampa `0 passed, 0 total` e passa. `wdio.conf.js`
  ha già la guardia `EXPECTED_TESTS`, ma il passo è `continue-on-error`, quindi il suo codice di
  uscita viene inghiottito e il verdetto legge solo il testo.

Insieme a `continue-on-error: true` su tutti e sette i passi, questo è precisamente la
configurazione che le regole del progetto chiamano peggiore di nessun check. Il `continue-on-error`
in sé è difendibile qui, perché una run diagnostica che raccoglie tutti i fallimenti invece di
fermarsi al primo è utile e il verdetto finale è il vero gate. Ma è difendibile **solo finché il
verdetto è solido**, e adesso non lo è.

Correzione, applicata in questo stesso branch: smettere di leggere il testo. Ogni check gira
attraverso `.github/scripts/e2e-check.sh`, che scrive il proprio codice di uscita in
`ci-logs/<nome>.exit`, e `.github/scripts/e2e-verdict.sh` pretende l'insieme atteso di check con
uno zero per ciascuno. Un nome atteso che non ha riportato è rosso, un nome che ha riportato e non è
nell'elenco è rosso, così un passo e l'elenco non possono divergere in silenzio.

### 3.3 I due fallimenti reali non sono quelli che ci aspettavamo

- **close-gate.** Ogni passo a livello X passa: "the dialog is mapped, not just present in the
  tree", "discard exited the app with status 0", "no process survived save", "the save branch's
  dialog is mapped". Poi fallisce su
  `save wrote the edit and moved nothing else / blocks before 3, after 3, differing []`
  (`e2e/scripts/close-gate-check.js:364`). `differing` vuoto significa che il file salvato è identico
  byte per byte a quello di prima. Il save non ha scritto niente. È una domanda su prodotto o
  modello, non sull'input.
- **late-edit.** `X Error: 9: Bad Drawable / ResourceID 0x200003 / xwininfo: No such window`, dopo
  che "the gate is mapped" era già passato. La harness ha tenuto un window id oltre la distruzione
  della finestra. È una terza classe: una race TOCTOU nella propria ispezione X. tine classifica
  esattamente questa firma (`BadWindow (invalid Window parameter)`) come infrastruttura ritentabile.

Nessuno dei due è `XSetInputFocus BadMatch` e nessuno dei due è timing di paint. Quei due sono già
chiusi, dai commit `458ecea` e `384f8af`, e i 33 test smoke che ci passano sopra sono verdi.

### 3.4 L'aritmetica dei bottoni del dialogo è corretta per fortuna

`e2e/lib/gtk-dialog.js` calcola
`x = dialog.absX + dialog.width - 24 - BUTTON_WIDTH/2 - slot*(BUTTON_WIDTH+12)` con
`BUTTON_WIDTH = 96`. Il commento ammette che è una stima. È peggio di una stima.

Layout reale misurato: GtkButtonBox rende i bottoni **contigui**, con larghezza
`(larghezza_dialogo - 16) / 3` e margini laterali di 8px. Confermato esatto a tre larghezze
(259 → 81, 346 → 110, 510 → 165). La harness assume 96 di larghezza, 12px di gap e 24px di margine
destro: sbagliato in tutti e tre i parametri. Esiti misurati con la formula reale:

| Corpo del messaggio                | Larghezza | Esito                                                             |
| ---------------------------------- | --------- | ----------------------------------------------------------------- |
| "This subtitle has unsaved edits." | 259       | chiedere Discard clicca **Save**; chiedere Save non clicca niente |
| `CLOSE_UNSAVED_BODY` reale         | 346       | tutti e tre centrati                                              |
| corpo di lunghezza tedesca         | 510       | chiedere Save clicca **Discard**                                  |

Risolvendo il modello, i tre bottoni sono centrati solo per larghezza in **[296, 436)**. Sweep dei
font sul corpo reale: Cantarell 11 → 361, Liberation Sans 11 → 357, DejaVu Sans 11 → 411,
DejaVu Sans 12 → **433**, tre pixel dal precipizio, DejaVu Sans 14 → 496, rotto. Un runner senza
`fonts-cantarell` ricade su DejaVu. E CLAUDE.md §9 impone stringhe pronte per i18n: la prima
traduzione può spostare la larghezza fuori banda da sola.

Onestà: questi sbagli falliscono **rumorosamente**, perché ogni ramo prova la propria risposta per
effetto. È fragilità latente di CI, non un buco silenzioso sui dati.

### 3.5 I tetti di attesa sono seduti sul cold start misurato

Nella run 33360311284, il tempo fra l'inizio del passo e la prima riga di log dell'app è
05:31:07.42 → 05:31:37.68 e 05:31:52.85 → 05:32:22.98: **circa 30 secondi**, subito dopo
`libEGL warning: DRI3 error: Could not get DRI3 device`. I tetti della harness sono 30000 ms
(`e2e/lib/driver.js:64` e `:98`, `e2e/lib/applog.js:35`), mocha timeout 60000 e
`waitforTimeout: 20000` (`e2e/wdio.conf.js:43,46`). Sono esattamente sopra il numero osservato.
`vercel-labs/native` usa `ready_timeout_ms=90000` per la stessa ragione e annota che i runner
condivisi si fermano ~27 s prima del primo evento di runtime.

### 3.6 Cosa è davvero inerente a Xvfb senza window manager

Separato da quello che sembra esserlo e non lo è.

**Inerente, e reale:**

- Senza WM il focus di input di default è **PointerRoot**. Misurato. I tasti seguono il puntatore,
  non una finestra. Finché la harness muove il puntatore prima di digitare funziona, ma è una
  proprietà su cui si sta appoggiando senza dirlo.
- Le finestre override-redirect (menu, popover, alcuni popup di toolkit) non ricevono mai focus da
  tastiera. È il caso documentato da `vercel-labs/native`. Se in M2.0 arriva un menu contestuale o
  un file chooser da guidare con i tasti, quello non funzionerà senza WM.

**Non inerente, e già chiuso:**

- `XSetInputFocus` che risponde `BadMatch` non c'entra con il window manager. Riprodotto: smappando
  la finestra figlia e chiamando XSetInputFocus si ottiene esattamente
  `BadMatch: major_opcode = 42 (X_SetInputFocus)`, e la causa è solo che la finestra non è
  viewable. Il manuale Xlib dice esattamente questo e non nomina window manager. `xdotool` issue #184
  riporta lo stesso `BadMatch` **con openbox in esecuzione**. `e2e/lib/input.js:23` lo documenta già
  correttamente e attende `IsViewable`.
- Le coordinate assolute non sono a rischio da reparenting. Misurato sotto openbox reale con
  titlebar da 22px: `xdotool getwindowlocation` e `xwininfo` riportano valori **identici** su
  frame, client e finestra mpv. `xdo.c` chiama apposta `XQueryTree` più `XTranslateCoordinates`. E
  comunque `e2e/lib/x11.js` legge già la seconda coppia di offset di `xwininfo -root -tree`, che è
  quella assoluta, e `grep getwindowgeometry e2e/` dà **zero hit**.

### 3.7 Cosa è inerente al protocollo WebDriver

Il protocollo W3C non espone né lo stato di uscita del processo né la lista dei sopravvissuti. Gli
script Node (`shutdown-check.js`, `close-gate-check.js`) sono la scelta giusta e sono quello che fa
chiunque tenga a queste proprietà: `Alexays/Waybar` ha `test/smoke/lifecycle.sh` in bash per la
stessa ragione dichiarata (SIGTERM non esegue i distruttori, le classi di crash che contano vivono
nel percorso create/destroy), `unslothai/unsloth` guida tutto da uno script Python di 850 righe che
parla WebDriver su HTTP grezzo, `vercel-labs/native` da 278 righe di bash. Questa parte della forma
di Sublore non ha bisogno di difese.

### 3.8 La superficie video non ha precedenti e AT-SPI non la coprirà mai

`src-tauri/src/video/surface/linux.rs` costruisce la superficie con `gdk::Window::new` più
`child.ensure_native()`. È una GdkWindow nuda, non un GtkWidget. L'accessibilità in GTK3 si aggancia
a GtkWidget (`gtk_widget_get_accessible`), e `grep -rln "Atk|accessible" /usr/include/gtk-3.0/gdk/`
dà zero file. Quindi non è solo la finestra estranea di mpv a essere invisibile ad AT-SPI: **anche
la superficie di Sublore lo è**, per lo stesso motivo. E `ldd /usr/bin/mpv` elenca 324 librerie,
zero delle quali atk, atspi o gtk.

Prova decisiva: riprodotto lo shape esatto (toplevel 1024x700, GdkWindow nativa 1024x400+0+49, mpv
attaccato via `--wid`), poi nascosta la superficie. `xwininfo` ha visto
`IsViewable → IsUnMapped`. Il dump AT-SPI prima e dopo era **identico byte per byte**. Quella è
l'assertion di `video-surface.spec.js:183`, e AT-SPI non la sa fare.

Stessa cosa per uscita e sopravvissuti: `org.a11y.atspi.Application` espone solo
GetApplicationBusAddress, GetLocale, AtspiVersion, Id, InterfaceVersion, ToolkitName, ToolkitVersion,
Version. Dopo un SIGKILL, la shell padre ha visto status 137 e AT-SPI ha visto solo "un'applicazione
ha lasciato il desktop", indistinguibile da un'uscita pulita, mentre due processi mpv orfani
restavano in vita completamente invisibili.

### 3.9 tauri-driver non ha manutenzione attiva

Versione pubblicata 2.0.6, metadata crates.io aggiornata **2026-05-06**, nessun rilascio da quasi
quattro mesi. Aperte e non mergiate: `#15605` (strip BiDi `webSocketUrl`, dal 2026-06-29), `#15935`
(BiDi più sintesi del campo `text` per Send Keys più readiness, dal 2026-08-29), più il fix di
readiness che vive solo sul branch `fix/wait-webdriver` citato in `#15156`. Difetti noti che
toccano Sublore: `#15156` (tauri-driver accetta connessioni TCP prima che il driver nativo sia
pronto), `#3576` (aperta dal 2022, non fa reaping affidabile del driver nativo fra sessioni,
workaround documentato `killall WebKitWebDriver`), `#15415` (WebdriverIO 9 manda `webSocketUrl: true`
e tauri-driver lo inoltra tale e quale, si risolve lato client con
`'wdio:enforceWebDriverClassic': true`).

Il repository tauri stesso non ha **nessuna** copertura CI di WebDriver: i suoi 21 workflow sono
audit, bench, fmt, lint, test-core, test-cli, test-android e release. Nessuno a monte sta difendendo
questo percorso.

### 3.10 Un dettaglio piccolo con conseguenze

`e2e/README.md` documenta `EXPECTED_TESTS = 30` come guardia anti-zero-test e chiede di aggiornarlo
a mano quando si aggiunge o si toglie un test. La run corrente ne esegue 33. La guardia funziona
(fallisce se ne passano meno di 30), ma il numero è già scollato dalla realtà e continuerà a
scollarsi. Non è urgente, va notato.

### 3.11 Windows sta messo peggio, e il blocco non è nostro

Tre progetti indipendenti hanno colpito lo stesso muro: i runner Windows ospitati girano come host
elevato, e WebView2 Runtime 150 ha tolto `--remote-debugging-port` per gli host elevati, quindi i
driver basati su msedgedriver non ottengono mai l'handshake DevTools. `webdriverio/desktop-mobile`
mette Windows in allow-fail per entrambi i provider esterni e linka la propria issue #542;
`fstubner/netscli` ha tolto il trigger `pull_request` annotando che nove run su dieci fallivano allo
stesso passo e che "un check che non può passare insegna a tutti a ignorare una X rossa";
`bloknayrb/tandem` è passato a dispatch-only il 2026-08-08; `jasonulbright/Spectra-PDF` ha disattivato
101 spec con `if: false` tenendoli come gate locale di release.

Nessuno l'ha risolto. Il milestone di attivazione Windows va pianificato su un runner self-hosted
non elevato, oppure sul provider embedded (§4), non su runner ospitati.

---

## 4. Le opzioni, con il prezzo

In ordine di quanto della suite di oggi sopravvive. Il prezzo è cosa smette di essere provato.

### 4.1 Tenere e indurire

Sopravvive **tutto**. Non si smette di provare niente.

Lavoro, in ordine di costo crescente:

1. Sistemare il grep del Verdict (§3.2). Mezz'ora. Senza questo, tutto il resto è costruito su un
   gate che può mentire.
2. Sistemare la race sul window id in late-edit: rileggere l'id, o trattare `Bad Drawable` su una
   finestra che ci si aspetta distrutta come la condizione attesa invece che come errore.
3. Diagnosticare close-gate come domanda di prodotto: perché il save scrive un file identico.
4. Alzare i tetti da 30000 a 90000 in `driver.js` e `applog.js` (§3.5). Il cold start misurato è 30 s.
5. Aggiungere `'wdio:enforceWebDriverClassic': true` alle capabilities (§3.9, tauri#15415) e un
   `killall WebKitWebDriver` prima della run (#3576).
6. Riscrivere `e2e/README.md:144` e `:107` con la piattaforma attaccata (§3.1).

Rischi accettati: PointerRoot resta il modello di focus (§3.6), l'aritmetica del dialogo resta
corretta per fortuna (§3.4), tauri-driver resta una dipendenza senza cadenza di rilascio (§3.9).

### 4.2 Aggiungere un window manager alla CI

Sopravvive **tutto**, e si guadagna la capacità di guidare popup override-redirect da tastiera.

Prezzo, che non è teorico:

- Non c'è precedente verde. L'unico progetto che gira questa combinazione (tine) non ha mai avuto
  una run Linux verde su CI (§2.3), e usa openbox solo su 4 scenari su ~60.
- Openbox sotto l'Xvfb annidato di GitHub introduce una classe di flake propria, documentata da
  tine: `_NET_ACTIVE_WINDOW` che punta a un frame distrutto, mitigato con
  `E2E_ALLOW_SYNTHETIC_FOCUS=1` e un retry infrastrutturale. Cioè si compra focus reale pagando con
  una scorciatoia sul focus.
- Il reparenting non rompe le coordinate di Sublore (§3.6, misurato), quindi l'argomento tecnico più
  citato **contro** il WM non vale, ma nemmeno l'argomento più citato **a favore** (BadMatch) vale.
- Non risolverebbe nessuno dei due fallimenti attuali.

L'unico argomento onesto a favore è preventivo: se M2.0 porta un file chooser o un menu contestuale
da guidare con i tasti, servirà. Allora la forma giusta è quella di tine: WM per scenario, non per
suite.

### 4.3 Sostituire l'aritmetica del dialogo con AT-SPI

Sopravvive **tutto**, e si elimina la fragilità di §3.4 e il punto hardcoded
`FIRST_CUE_TEXT {x:750, y:540}` in `close-gate-check.js` (il cui commento dice già "M2.0 must
revisit this").

Provato end to end, non dedotto. Sotto Xvfb senza WM, sullo stesso dialogo GTK3 di
`src-tauri/src/dialog.rs`:

```
'alert' name='Warning'
  'button' name='Save'    [347,375 110x37]
  'button' name='Discard' [457,375 110x37]
  'button' name='Cancel'  [567,375 110x37]
```

Nomi accessibili più extent in coordinate schermo. Localizzato per nome, letto l'extent, cliccato il
centro **con XTEST**: GTK ha risposto correttamente per tutti e tre (-8 YES, -9 NO, -6 CANCEL).
L'input resta reale, non diventa `do_action`, quindi non si indebolisce niente.

Effetto collaterale rilevante: quella prova **non ha fatto nessuna chiamata a `focusWindow`**.
`clickDialogButton` oggi chiama `focusWindow(dialog.id)` prima di un click puramente puntatore
(`gtk-dialog.js:20`), cioè dipende proprio dall'operazione che §3.6 dice essere impossibile per un
popup senza WM. Togliere quella chiamata è più piccolo di qualsiasi altra opzione qui.

Sulla webview: verificato sul binario vero. Un solo albero, `frame name='Sublore'` →
`document web` → `list box name='Cues' [288,530 736x170]` più ogni bottone e campo con coordinate
schermo, popolato entro 1.5 s dalla comparsa dell'app sul bus. Quindi "la lista cue ha dipinto"
diventa un osservabile su cui attendere, che è esattamente il pattern che ha già chiuso i due bug
noti.

Correzione a una fonte che circola: il meccanismo del bridge ATK descritto da mariospr è del 2013 ed
è obsoleto. WebKitGTK 2.36 (marzo 2022) ha sostituito ATK con un'implementazione AT-SPI diretta su
D-Bus; il sorgente WebKit ha `Source/WebCore/accessibility/atspi/` e nessuna directory `atk`. La
conclusione regge, il meccanismo no. Il floor di versione è gratis: `libwebkit2gtk-4.1`, già in
`ci.yml`, parte da 2.36.

Prezzo: non è a costo zero di configurazione. Il registry AT-SPI è attivato da systemd
(`/usr/lib/systemd/user/at-spi-dbus-bus.service`) e in CI non c'è sessione systemd. Il primo
tentativo è fallito con "Could not activate remote peer 'org.a11y.atspi.Registry'". Servono tre
pezzi in più: pacchetto `at-spi2-core`, wrapper `dbus-run-session`, e lancio esplicito di
`at-spi-bus-launcher --launch-immediately` e `at-spi2-registryd`. Oggi il job gira `xvfb-run` nudo
senza session bus. Precedente verde: `aws-deadline/deadline-cloud`, 12 run su 12 (§2.6), ma su
Qt/PySide6, quindi la metà GTK+WebKit è provata solo su questa macchina e va provata su
ubuntu-latest con una run usa e getta prima di riscrivere niente.

AT-SPI **non** sostituisce `x11.js` né gli script di processo (§3.8). È un'aggiunta al percorso del
dialogo e della cue list. Migliorerebbe anche `findToplevel`, perché il frame accessibile è
`name='Sublore' 1024x700+0+0` e la finestra group-leader da 10x10 che oggi costringe a selezionare
per geometria non compare affatto nell'albero AT-SPI.

### 4.4 Passare a WebDriver per i click su CI

Sopravvive **tutto** in termini di assertion, ma cambia natura a quello che si prova.

Cosa si guadagna: si elimina il percorso xdotool per i click DOM, che è dove vivono PointerRoot e
il focus. Precedente verde: gitbutler e webdriverio (§2.1).

Cosa si perde, ed è il prezzo che un riassunto nasconderebbe:

- **La verifica si spacca fra piattaforme.** Gli endpoint funzionano su ubuntu-latest e non su
  Fedora 44 (§3.1). Oggi la stessa harness gira identica sulla macchina del proprietario e su CI, ed
  è per questo che vale come verifica comportamentale. Con i click via WebDriver, o si tiene un
  doppio percorso condizionato alla distribuzione (due percorsi, due comportamenti, un solo
  verdetto) o la verifica locale smette di essere la stessa cosa che gira in CI. Fino a quando il fix
  Fedora non arriva, questo è un costo reale.
- **Non si guadagna la digitazione.** Send Keys non è provato verde da nessuno ed è già rotto su
  WebKitWebDriver 2.52.3 (§3.1). Quindi si finirebbe come gitbutler, che scrive `input.value` da
  `browser.execute`. Quella è la differenza fra input reale e evento DOM sintetico, ed è
  esattamente il tipo di indebolimento che le regole del progetto vietano: la suite smetterebbe di
  provare che digitare nell'editor funziona per un utente e proverebbe solo che il modello reagisce a
  un valore assegnato.
- L'input puntatore reale resta comunque necessario per il dialogo GTK e per qualunque superficie
  nativa, quindi xdotool non sparisce, si aggiunge un secondo meccanismo.

### 4.5 Provider embedded (@wdio/tauri-service)

Sopravvive **tutto**, e in teoria è l'unica opzione uniforme fra Fedora e CI.

`webdriverio/desktop-mobile` ha reso `embedded` il default con l'ADR 0002 del 2026-08-03, proprio per
smettere di dipendere dai driver di piattaforma. `tauri-plugin-webdriver` implementa in-process
`element/send_keys` (`router.rs:86`) e `/session/{id}/actions` (`router.rs:210-211`), quindi è
strutturalmente immune sia allo split GTK3/GTK4 sia ai problemi di focus del driver esterno. Sarebbe
anche l'unica strada che sblocca il porting macOS più avanti.

Prezzo, che oggi lo esclude:

- Bug aperto `webdriverio/desktop-mobile#591`: la sessione si blocca permanentemente dopo qualunque
  click il cui handler chiami `invoke()`. Riproduzione al 100%, ancora aperto al 2026-08-31. In
  Sublore quasi ogni click invoca un comando Tauri.
- Adozione zero: `"@wdio/tauri-service" path:package.json` dà **0 hit** su tutto GitHub.
- Richiede un plugin Tauri nel binario, cioè codice di test dentro il prodotto.

Da tenere d'occhio per il milestone Windows, non da adottare adesso.

### 4.6 Splittare fra CI e macchina del proprietario

Sopravvive quello che si tiene in CI. Il progetto ha già questo pattern e lo documenta bene:
`pnpm e2e:webview` e `pnpm e2e:wayland` sono già check da macchina del proprietario, con il motivo
scritto in `ci.yml` e il rifiuto esplicito a girare senza prerequisito invece di passare per il
motivo sbagliato. Quella è la forma giusta.

Prezzo se si sposta close-gate e late-edit su macchina del proprietario: smette di essere provato in
CI che le modifiche non salvate non vengano perse silenziosamente, che è la garanzia di sicurezza
dati di CLAUDE.md §3, cioè la cosa più importante che la suite prova. Sarebbe provata solo quando
qualcuno la gira a mano. Da non fare a meno che i due check non si rivelino irriducibilmente
non deterministici, e oggi non lo sono: falliscono in modo deterministico e per cause specifiche.

Se un giorno si arriva a questo, la disciplina da copiare è quella di tine: un registro versionato
di quarantena, con motivo datato per scenario e condizione scritta per tornare a bloccare, non un
`continue-on-error` sul job. E il criterio di riammissione, sempre di tine: due run locali pulite
consecutive sullo stesso binario più tre run ospitate pulite consecutive.

### 4.7 Abbandonare l'E2E interattivo per uno smoke più piccolo

Sopravvive poco. È quello che fanno quasi tutti (§2.1): gitbutler gatea su un test,
`Felix-LeeSM/table-view` è verde perché guida quasi tutto con `browser.execute`, gptme guida un
mock scritto dal workflow.

Prezzo: smette di essere provato tutto quello per cui questa harness esiste. Il gate di chiusura con
i suoi tre rami, il fatto che due dei tre terminino il processo, l'assenza di sopravvissuti, la
superficie video attaccata, la scala intera applicata una volta sola. Resterebbe "l'app parte e la
webview risponde".

E va detto senza giri: adottare questa opzione **mantenendo** i check nel repository ma marcandoli
advisory è la variante che nasconde i fallimenti. È letteralmente screenpipe (§2.2): zero successi
su cento run, invisibile, e nessuno che debba sistemarlo. Se si sceglie di abbandonare, i check si
tolgono, non si etichettano.

---

## 5. Raccomandazione

**Tenere e indurire (4.1), poi AT-SPI per il dialogo (4.3).** Nient'altro adesso.

Il motivo è che i dati non sostengono un cambio di forma. Cinque check su sette sono verdi su
ubuntu-latest oggi, inclusi 33 test che fanno doppio click e digitano via XTEST reale. I due rossi
sono un save che scrive un file identico e una race sul window id: entrambi specifici, entrambi
diagnosticabili, nessuno dei due sintomo della forma della harness. I due fallimenti che erano
sintomo della forma sono già stati chiusi, e sono stati chiusi con lo stesso metodo entrambe le
volte, aspettando un osservabile invece di un ritardo. Quel metodo funziona.

Ordine di lavoro:

1. **Il grep del Verdict** (§3.2). Prima di tutto. Un gate che può dichiarare verde una run rossa
   invalida ogni misura successiva, ed è il caso che le regole del progetto chiamano peggiore di
   nessun check.
2. La race sul window id in late-edit.
3. Il save di close-gate, come domanda di prodotto.
4. I tetti a 90 s (§3.5), `enforceWebDriverClassic` e `killall WebKitWebDriver` (§3.9).
5. La piattaforma attaccata a `e2e/README.md:144`, e la correzione del pacchetto Fedora a `:107`
   (§3.1).
6. Una run CI usa e getta che prova solo l'infrastruttura AT-SPI su ubuntu-latest (§4.3). Se passa,
   sostituire `gtk-dialog.js` e il punto hardcoded della cue list.

Cosa **non** fare adesso: non aggiungere un window manager (nessun precedente verde, non risolve
nessuno dei due rossi, porta flake propria), non spostare i click su WebDriver (spacca la verifica
fra Fedora e CI e non dà la digitazione), non adottare il provider embedded (#591 lo esclude), non
marcare niente advisory.

### Cosa cambierebbe la raccomandazione

- **Se dopo il punto 3 la suite diventa intermittente invece che deterministicamente rossa**, cioè
  se gli stessi check falliscono a volte su codice invariato per dieci run consecutive, allora la
  diagnosi cambia: non è più prodotto, è la forma. In quel caso la mossa è 4.6, con il registro di
  quarantena di tine, e il WM per gli scenari del dialogo soltanto.
- **Se M2.0 introduce un file chooser nativo o un menu contestuale da guidare con i tasti**, il WM
  per scenario (4.2) diventa necessario, perché senza WM i popup override-redirect non ricevono mai
  focus da tastiera (§3.6). Non serve deciderlo prima.
- **Se ubuntu-latest migra a 26.04**, va riverificato tutto il percorso di digitazione: tauri#15871
  dice che Send Keys si rompe lì, e anche se oggi Sublore non lo usa, quella migrazione porta anche
  un WebKitWebDriver diverso sotto la stessa etichetta.
- **Quando il fix Fedora arriva** (RH 2493782, chiuso upstream il 2026-08-21), gli endpoint WebDriver
  diventeranno disponibili anche in locale, e l'obiezione principale a 4.4 (la verifica che si
  spacca fra piattaforme) cade. A quel punto vale la pena rimisurare, non prima.
- **Per il milestone Windows**, niente di quanto sopra si applica. Il blocco è esterno e nessuno dei
  quattro progetti che l'ha colpito l'ha risolto (§3.11). Va pianificato su runner self-hosted non
  elevato oppure sul provider embedded, e va pianificato presto, perché è la condizione di uscita
  che gatea la vendita.

---

## Nota su cosa è verificato e cosa no

Verificato girandolo: gli endpoint WebDriver su questa macchina (sonda HTTP diretta contro il
binario vero), il packaging Fedora e Ubuntu (`rpm -qf`, container noble), il bug del grep del
Verdict (riprodotto qui sopra), il layout GtkButtonBox e lo sweep dei font, il percorso AT-SPI end to
end incluso il click XTEST e le tre risposte GTK, l'invisibilità della superficie video ad AT-SPI
(dump identico byte per byte a fronte di `IsViewable → IsUnMapped`), il BadMatch di XSetInputFocus,
l'equivalenza fra `xdotool getwindowlocation` e `xwininfo` sotto openbox con reparenting, il cold
start di ~30 s dai timestamp della run.

Verificato leggendo repository e run reali: tutti i conteggi di §2, con nomi di file e id di run.

**Non** verificato: qualunque comportamento su Windows o macOS; il funzionamento di AT-SPI
sull'albero GTK+WebKit su ubuntu-latest (provato solo su Fedora 44); Element Send Keys attraverso
tauri-driver su qualunque piattaforma, perché nessun progetto verde lo esercita.
