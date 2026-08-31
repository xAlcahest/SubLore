# Windows: `STATUS_ENTRYPOINT_NOT_FOUND` sui tre target che toccano mpv

Diagnosi del 2026-08-31, ramo `fix/save-writes-what-it-says`. Stato: causa isolata offline, conferma
sul runner in corso. Nessuna affermazione qui dentro riguarda il comportamento dell'app su Windows:
la piattaforma compila, non è mai stata eseguita (CLAUDE.md §5.5).

## Il sintomo

`cargo test --workspace --no-fail-fast` su `windows-latest` fa uscire tre eseguibili di test con
`0xc0000139` prima di stampare una sola riga:

- `crash_main_thread`
- `crash_safety`
- `video_playback`

Gli altri quattro target in `src-tauri/tests/` passano. `0xc0000139` è
`STATUS_ENTRYPOINT_NOT_FOUND`: il loader ha trovato una DLL dipendente ma non un simbolo che
qualcuno le chiede. Non è `0xc0000135` (`DLL_NOT_FOUND`), quindi la DLL c'è ed è incompleta.

## Perché solo tre target su sette

Il linker MSVC non mette nella import address table i simboli che nessuno referenzia. I quattro
target che passano non nominano nessuna funzione mpv, quindi il loro eseguibile non importa
`libmpv-2.dll` e non ne eredita le dipendenze. I tre che falliscono la importano.

## Cosa è stato escluso

Il primo passo diagnostico confrontava i simboli mpv importati dall'eseguibile con quelli esportati
dalla DLL messa accanto: 54 export, 11 import, nessuno mancante. La risposta ovvia era sbagliata, e
il valore di quel passo è stato togliere di mezzo l'ipotesi comoda invece di confermarla.

## La causa

`libmpv-2.dll` non è autosufficiente: importa staticamente 47 DLL, e una di queste non fa parte del
corredo garantito di Windows.

    vulkan-1.dll (12 simboli importati staticamente):
      vkCreateDisplayPlaneSurfaceKHR, vkCreateWin32SurfaceKHR, vkDestroySurfaceKHR,
      vkEnumeratePhysicalDevices, vkGetDisplayModePropertiesKHR,
      vkGetDisplayPlaneSupportedDisplaysKHR, vkGetInstanceProcAddr,
      vkGetPhysicalDeviceDisplayPlanePropertiesKHR, vkGetPhysicalDeviceDisplayPropertiesKHR,
      vkGetPhysicalDeviceProperties, vkGetPhysicalDeviceProperties2,
      vkGetPhysicalDeviceQueueFamilyProperties2

Import statici, non `GetProcAddress`: devono risolversi al caricamento del processo anche se mpv gira
headless e non tocca mai la GPU. I nostri test lo fanno — `video_playback.rs` costruisce il core con
`vo=null, ao=null` — ma il loader non lo sa e non gliene importa.

Le altre 46 DLL sono tutte di sistema e presenti su un'immagine Server: `AVICAP32`, `AVRT`,
`bcryptprimitives` (`ProcessPrng`, Windows 10+), `d2d1`, `DWrite`, `dwmapi`, `IPHLPAPI`, `Normaliz`,
`OPENGL32`, `Secur32`, `SETUPAPI`, `SHCORE`, `UxTheme`, `WLDAP32` e il resto del corredo standard.
`vulkan-1.dll` è l'unica che dipende da cosa ha installato il driver grafico, e su un runner senza GPU
il loader di sistema — quando c'è — è vecchio o parziale. Un loader vecchio esporta
`vkGetPhysicalDeviceProperties` ma non `vkGetPhysicalDeviceProperties2` né
`vkGetPhysicalDeviceQueueFamilyProperties2`, che sono core Vulkan 1.1: DLL presente, entry point
mancante, cioè esattamente il codice che vediamo.

L'evidenza sopra è stata ricavata in locale sull'archivio effettivamente pinnato in CI
(`mpv-dev-x86_64-20260814-git-7b8915bc1d.7z`, sha256 verificato) con `objdump -p`, non su un archivio
somigliante.

## Il rimedio in CI

Stesso schema già usato per libmpv: si mette accanto ai binari di test un loader Vulkan pinnato, così
la directory dell'eseguibile vince sulla ricerca di sistema e il grafo si risolve in modo
deterministico invece di dipendere da cosa l'immagine del runner si porta dietro quel mese.

Il loader è quello di riferimento Khronos/LunarG (`VulkanRT-1.3.296.0-Components`, Apache-2.0,
compatibile GPL), scaricato con checksum. Verificato offline: esporta 246 simboli `vk*`, fra cui
tutti e 12 quelli che libmpv importa. Le sue uniche dipendenze sono `KERNEL32`, `ADVAPI32` e
`CFGMGR32`, tutte di sistema.

Non è un finto passaggio: è il loader vero, e senza ICD installati non trova nessun device — che è
la condizione in cui i test girano comunque, dato che sono headless.

## Cosa questo dice del prodotto, non della CI

Una macchina Windows senza driver Vulkan ha lo stesso grafo di dipendenze del runner. Se un utente
apre Sublore su un portatile con il solo Microsoft Basic Display Adapter, `libmpv-2.dll` non si
carica e l'app muore all'avvio senza messaggio. Non è un problema della CI ed è la CI ad averlo
trovato. Va risolto nel milestone di attivazione Windows, non qui: vedi BACKLOG N7.
