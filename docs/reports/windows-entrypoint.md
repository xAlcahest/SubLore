# Windows: `STATUS_ENTRYPOINT_NOT_FOUND` sui tre target che toccano mpv

Diagnosi del 2026-08-31, ramo `fix/save-writes-what-it-says`. Riguarda la CI, non il comportamento
dell'app: su Windows Sublore compila e non è mai stato eseguito (CLAUDE.md §5.5).

## Il sintomo

`cargo test --workspace --no-fail-fast` su `windows-latest` fa uscire tre eseguibili di test con
`0xc0000139` prima che stampino una riga:

    Caused by:
      process didn't exit successfully: ...\crash_main_thread-6449b479a3a26ef0.exe
      (exit code: 0xc0000139, STATUS_ENTRYPOINT_NOT_FOUND)

Gli altri quattro target in `src-tauri/tests/` passano, e passa tutto il resto del workspace.
`0xc0000139` non è `0xc0000135` (`DLL_NOT_FOUND`): la DLL c'è, le manca un simbolo.

## Prima ipotesi, sbagliata: i simboli mpv

Il primo passo diagnostico confrontava i simboli mpv importati dall'eseguibile con quelli esportati
dalla DLL messa accanto. Risultato: 54 export, 11 import, nessuno mancante. L'ipotesi comoda era da
buttare.

## Seconda ipotesi, sbagliata: Vulkan

`libmpv-2.dll` importa **staticamente** 47 DLL, e una non fa parte del corredo garantito di Windows:
`vulkan-1.dll`, da cui prende 12 simboli fra cui `vkGetPhysicalDeviceProperties2` e
`vkGetPhysicalDeviceQueueFamilyProperties2`, che sono core Vulkan 1.1. Import statici, non
`GetProcAddress`: devono risolversi al caricamento anche se mpv gira headless — e i nostri test lo
fanno, `video_playback.rs` costruisce il core con `vo=null, ao=null`. Su un runner senza GPU un
loader Vulkan vecchio o parziale avrebbe dato esattamente il codice che vediamo.

Era una spiegazione coerente, completa e falsa. Il runner ha risposto
`vulkan-1.dll : 12 imports, all present`. È il motivo per cui la correzione non è stata scritta prima
della verifica: era già pronta, con il loader Khronos/LunarG pinnato e i 12 simboli controllati
offline, e sarebbe stata una toppa su un problema inesistente.

## Perché il confronto delle tabelle non poteva rispondere

Lo stesso passo diagnostico dichiarava mancanti `kernel32!EnterCriticalSection`,
`ole32!CoInitializeEx`, `user32!DefWindowProcW` e altri quaranta. Sono tutti presenti: sono **export
forwardati**, che `dumpbin /exports` elenca senza RVA, e la regex li saltava. Nella stessa lista
comparivano `api-ms-win-crt-*.dll : NOT FOUND`, che non sono file ma contratti API set risolti dal
loader.

Un confronto di tabelle statiche non sa niente di forwarder e di API set. La domanda va fatta al
loader: `LoadLibrary` + `GetProcAddress` seguono i forwarder, risolvono gli API set e rispondono
sulla DLL che il processo otterrebbe davvero. Da qui `.github/scripts/windows-entrypoint.ps1`.

## La causa

In mezzo ai falsi positivi ce n'era uno vero:

    comctl32.dll (C:\Windows\System32\comctl32.dll): MISSING TaskDialogIndirect

Non è un forwarder. `C:\Windows\System32\comctl32.dll` è la versione 5.82, tenuta lì per
compatibilità, e `TaskDialogIndirect` non esiste in 5.82: sta nella versione 6, che vive in WinSxS e
si ottiene solo se il binario porta un manifest che dichiara la dipendenza da
`Microsoft.Windows.Common-Controls` 6.0.0.0.

`tauri_build::build()` quel manifest lo mette, ma sul binario dell'applicazione. I binari dei test di
integrazione li produce cargo per conto suo e non lo ricevono. Chi importa `TaskDialogIndirect` è la
via dei dialoghi nativi — `crash::show_dialog` e `project::choose_path` passano dal plugin, quindi da
rfd — e i tre target che falliscono sono i tre che linkano quel codice. I quattro che passano non lo
referenziano, il linker MSVC non mette in tabella un import che nessuno usa, e il loader non ha
niente da cercare.

Le due cose viaggiano insieme per caso: sono anche i tre target che linkano mpv, ed è per questo che
la prima ipotesi sembrava reggere.

## Il rimedio

`src-tauri/build.rs` passa ai soli target di test lo stesso manifest che l'app ha già:

    cargo:rustc-link-arg-tests=/MANIFEST:EMBED
    cargo:rustc-link-arg-tests=/MANIFESTINPUT:<src-tauri/windows-tests.manifest>

Il manifest dichiara solo la dipendenza da Common-Controls 6.0.0.0. `mt.exe` lo fonde con quello che
rustc già incorpora, quindi non sostituisce niente. Il test carica come carica l'app, che è la
condizione che vogliamo testare.

## Cosa resta non verificato

Che i tre target passino su Windows lo dirà la CI. Che il comportamento che testano sia corretto su
Windows non lo dice nessuno: la suite comportamentale gira su Linux e su Windows non è mai stata
eseguita. Questo è il milestone di attivazione Windows, non questo lavoro.
