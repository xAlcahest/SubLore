# Sicurezza del repository: cosa fanno davvero i progetti seri, e cosa conviene a Sublore

Ricerca del 2026-08-30 / 2026-08-31. Sublore e' pubblico da poco, il lavoro lo dirige una persona
sola tramite agenti, la CI gira su Linux e Windows, la licenza e' GPL-3.0 e il confine con i moduli
chiusi deve ancora arrivare. Ogni raccomandazione qui dentro e' pesata su questo, non su un team.

## Come e' stato costruito il campione

Cinque ricercatori hanno letto file veri con `gh api`, non guide. I campioni si sovrappongono ma non
coincidono, quindi ogni conteggio qui sotto porta il suo denominatore e il suo campione. Nessuno ha
usato `gh search repos --sort stars` grezzo come campione: in cima a quella lista ci sono liste
curate e roadmap per colloqui, che non hanno CI e non hanno postura da copiare. E' stata usata solo
come serbatoio per gli otto repo piu' stellati che spediscono software vero.

| Angolo                              | Repo aperti                                              | Materiale letto                                                              |
| ----------------------------------- | -------------------------------------------------------- | ---------------------------------------------------------------------------- |
| Hardening dei workflow              | 40 (2 scartati: kubernetes usa Prow, golang/go usa LUCI) | 671 file in `.github/workflows`, dependabot, `.github/zizmor.yml`, rulesets  |
| SAST, fuzzing, gate di linguaggio   | 36                                                       | 547 workflow, `Cargo.toml`, `deny.toml`, `fuzz/`, elenco progetti OSS-Fuzz   |
| Supply chain in entrata e in uscita | 35                                                       | 938 workflow (4,9 MB), lockfile, asset delle release, environment, rulesets  |
| Governance e impostazioni           | 40 + Sublore                                             | 121 rulesets letti come dati, environment, PVR, firma degli ultimi 30 commit |
| Automazione delle dipendenze        | 95                                                       | 40 `dependabot.yml` e 31 config Renovate, letti in pieno o grepati           |

Composizione voluta: shipper di binari desktop (zed, helix, alacritty, signal-desktop, mullvad,
keepassxc, bitwarden, obs-studio, syncthing, electron, vscode, localsend, spacedrive, lapce),
gestori di pacchetti e runtime (cargo, crates.io, npm/cli, pnpm, pip, brew, uv, node, cpython, deno,
bun), crittografia e supply chain (openssl, rustls, pyca/cryptography, RustCrypto, libsodium, ring,
sigstore/cosign, ossf/scorecard), parser di file ostili (image-rs, typst, mpv, whisper.cpp), piu'
lo stack esatto di Sublore (tauri, wry, tao, plugins-workspace).

### Visibile contro invisibile

Visibile dall'esterno e contato direttamente: workflow, `dependabot.yml`, config Renovate,
`deny.toml`, lockfile, nomi degli asset di release, `.immutable` sulle release, presenza di
SECURITY.md e CODEOWNERS.

Diventato visibile di recente, e sfruttato qui: `GET /repos/{o}/{r}/rulesets` risponde a un lettore
qualsiasi su repo pubblico, quindi nomi, target, pattern di ref, tipi di regola e numero di
approvazioni richieste sono dati, non congetture. Idem `GET /repos/{o}/{r}/environments` (nomi,
regole di protezione, revisori richiesti) e `GET /repos/{o}/{r}/private-vulnerability-reporting`.

Resta invisibile, e dove serve e' marcato come tale: la branch protection classica (l'unico segnale
pubblico e' il booleano `protected`), i bypass actor nei rulesets (tutti e 121 tornano un array
vuoto, cioe' redazione, non verita': electron e bitwarden fanno girare bot di release su ref
protetti), e tutto `security_and_analysis`, che e' admin-only. Su Sublore quest'ultimo si legge col
token dell'owner, quindi i numeri su Sublore sono misurati, non dedotti.

Nota di igiene sulle fonti aggregate: l'API di OpenSSF Scorecard ha restituito per curl/curl un
Token-Permissions 0 da una scansione del 2022-11-09, mentre oggi tutti e 16 i workflow di curl hanno
un blocco `permissions:`. Il badge e' vecchio e grossolano. Ogni numero qui viene dai file.

## 1. La risposta breve

Quattro cose, in ordine. Il resto del documento e' il supporto.

**1. Accendere le impostazioni di sicurezza del repo. Dieci minuti, mani dell'owner sulla UI.**
Misurato oggi su `xAlcahest/SubLore`: `secret_scanning` disabled, `secret_scanning_push_protection`
disabled, `dependabot_security_updates` disabled, private vulnerability reporting `{"enabled":false}`,
`branches/main` con `protected: false`, `/rulesets` vuoto, nessun SECURITY.md. Il repo e' appena
diventato pubblico: la push protection e' la versione meccanica della regola che CLAUDE.md §4 oggi
affida alla memoria di chi committa, e la memoria qui e' quella di un agente. Da fare a mano perche'
nessun file nel repo puo' farlo.

**2. Chiudere il buco piu' grande della CI: `dtolnay/rust-toolchain@stable`.** E' un branch mutabile,
non un tag, e gira prima di ogni build su ogni push e ogni PR. Chi controlla quel branch esegue
codice in un job che ha il repo in checkout. Nessun bot puo' aggiornare o fissare un riferimento a
branch. Assieme: `persist-credentials: false` sui checkout e zizmor come job di CI. Un'ora, la fa
un agente.

**3. Ruleset su `main` e sui tag. Minuti, mani dell'owner sulla UI.** Blocco force-push e
cancellazione su tutti i branch, PR obbligatoria su main con **zero** approvazioni richieste, tag
`v*` non cancellabili e non spostabili. Il ruleset con 0 approvazioni e' la forma pensata per chi
lavora solo: GitHub non permette all'autore di approvare la propria PR, quindi mettere 1 significa
auto-bloccarsi e finire per concedersi un bypass, che e' peggio di non aver messo niente.

**4. Sistemare `dependabot.yml`: c'e' un bug vivo.** `ignore: - dependency-name: libmpv2` senza
qualificatori sopprime anche gli aggiornamenti di sicurezza, quindi oggi un advisory su libmpv2 non
produrrebbe nessuna PR, in silenzio. Due righe. Piu' il cooldown a scaglioni. Minuti, la fa un agente.

Costo totale realistico: mezza giornata, di cui forse venti minuti effettivi dell'owner nella UI.

## 2. Cosa fanno i repository campionati

### 2a. Visibile nei file

**Pinning delle action per SHA.** Il conteggio dipende da cosa si conta. Sui 38 repo con Actions del
primo campione, 24 fissano per SHA il 100% delle action di terze parti; 11 lo fanno in parte o per
niente; 3 non usano action di terze parti (alacritty, vscode, rust-lang/rust). Sul campione da 36 la
stessa cosa vista sui riferimenti totali: 17 su 36 fissano il 90% o piu'. Esempi con file:
zed-industries/zed `.github/workflows/run_tests.yml:30`
(`step-security/harden-runner@9af89fc71515a100421586dfdb3dc9c984fbf411`), pnpm/pnpm
`.github/workflows/zizmor.yml:30` (`zizmorcore/zizmor-action@3dc1ecc9... # v0.6.2`), curl/curl
`.github/workflows/codeql.yml` (`actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1`),
microsoft/terminal (`msys2/setup-msys2@66cd2cce69caa17b53920067426061ca1de3a884 # v2.32.0`).

I controesempi contano piu' degli esempi, perche' sono lo stack di Sublore: **tauri-apps/tauri fissa
2 riferimenti su 117**, e `.github/workflows/test-core.yml:82` dice
`uses: dtolnay/rust-toolchain@master`, mentre `.github/workflows/supply-chain.yml` usa
`actions/checkout@master`. Non pinnano nulla nemmeno tokio-rs/tokio (0 su 176), rustls/rustls (0 su
70), npm/cli (0 su 133), RustCrypto/hashes (1 su 64), BurntSushi/ripgrep (0 su 7, con
`dtolnay/rust-toolchain@master`), mpv-player/mpv, godotengine/godot, image-rs/image.

Esiste una via di mezzo praticata e motivata per iscritto: syncthing/syncthing
`.github/workflows/build-syncthing.yaml` pinna 22 su 22 action di terze parti e lascia
`actions/checkout@v5` sul tag, con un commento in testa che dice che di GitHub ci si fida per forza
mentre agli altri autori si riserva cura, "specialmente nei percorsi che portano al codice
impacchettato e firmato". Stessa forma in helix-editor/helix (10 su 10 terze parti, 0 su 15
`actions/*`) e bitwarden/clients (43 su 43 terze parti).

**`permissions:` esplicito.** 35 su 38 nel primo campione, 33 su 36 nel secondo. La forma da copiare
e' `permissions: {}` in testa al file e lo scope minimo sul singolo job: neovim/neovim
`.github/workflows/labeler_pr.yml:6`, yt-dlp/yt-dlp `.github/workflows/label-handler.yml:8`,
astral-sh/uv `.github/workflows/release.yml`, mullvad/mullvadvpn-app `.github/workflows/rust-supply-chain.yml`
(56 workflow su 58). FiloSottile/age `.github/workflows/build.yml` e' l'esempio piu' compatto:
`contents: read` in testa, e solo il job `upload` dichiara `contents: write`, `attestations: write`,
`id-token: write`. Senza blocco in nessun file: alacritty, godotengine/godot, sharkdp/bat,
localsend, spacedrive.

**`persist-credentials: false` su checkout.** 23 su 38 e 19 su 35 nei due campioni che l'hanno
contato. python/cpython lo usa in 18 workflow, astral-sh/uv in 37, nodejs/node in 28, pnpm in 18,
sigstore/cosign in 15. Da pratica di nicchia e' diventata maggioranza perche' zizmor la segnala.
Non viaggia in automatico col pinning: zed pinna tutto e lo usa in 2 workflow su 49.

**zizmor.** Adozione fra 6 su 36 e 13 su 38 a seconda del campione, tutta recente: le versioni
fissate sono v0.5.7, v0.6.2, v1.29.0, tutte 2025-2026. Job dedicato in dani-garcia/vaultwarden
`.github/workflows/zizmor.yml` (31 righe, `permissions: {}` in testa), pnpm/pnpm
`.github/workflows/zizmor.yml`, neovim/neovim, astral-sh/uv `.github/workflows/check-zizmor.yml`,
jedisct1/libsodium. Dentro un job esistente in Homebrew/brew `.github/workflows/actionlint.yml:49`
(`zizmor --format sarif . > results.sarif`, caricato nella tab Security) e curl/curl
`.github/workflows/checksrc.yml:201`, che lo esegue due volte: `--persona pedantic` bloccante e
`--persona auditor` solo informativo. Via pre-commit in astral-sh/ruff, pypa/pip, python/cpython,
home-assistant/core.

**`pull_request_target`.** 13 su 38 lo usano, in 35 file. **Zero di quei 35 fa checkout della head
della PR.** E' il risultato piu' unanime di tutta la ricerca: elettron 7 file, zed 5, neovim 4,
node 4, bun 4, react 4, tutti solo per metadati (etichette, notifiche, backport). 25 su 38 non lo
usano affatto.

**Iniezione via template.** Grep su tutti e 671 i file per interpolazione diretta di
`pull_request.title`, `pull_request.body`, `comment.body`, `head_ref` dentro blocchi `run:`: nessun
caso non sicuro. Chi consuma testo controllato dall'attaccante lo passa da `env:`: neovim
`.github/workflows/labeler_pr.yml:26-33` (`PR_TITLE:` poi `"$PR_TITLE"`), npm/cli
`.github/workflows/pull-request.yml:51`, Homebrew/brew `.github/workflows/check-prs.yml:57,81`.

**CodeQL.** Molto piu' raro di quanto la reputazione suggerisca: 5 su 36 hanno un workflow CodeQL
(curl, nodejs/node, npm/cli, Homebrew/brew, bevyengine/bevy) e **1 solo repo Rust su 25 analizza
Rust**, bevyengine/bevy `.github/workflows/security-static-analysis.yml`, con matrice
`language: actions` e `language: rust`, entrambe `build-mode: none`. Verificato che non ci sia una
pila di "default setup" nascosti: interrogando `commits/{sha}/check-runs` su tutti e 36, l'unico che
esegue CodeQL in qualunque forma e' bevy. CodeQL per Rust e' andato in preview il 2025-06-30 e in
GA il 2025-10-14; su GitHub `languages: rust` compare in 84 workflow contro 898 per javascript.

**cargo-deny e cargo-audit.** 7 dei 25 repo Rust eseguono cargo-deny, 5 hanno un `deny.toml`
(bevy, rust-lang/cargo, rustls, tokio, image-rs; nell'altro campione anche atuin, lapce, mullvad,
servo). Il pattern da copiare e' in rust-lang/cargo `.github/workflows/audit.yml`: matrice
`advisories` e `bans licenses sources` con
`continue-on-error: ${{ matrix.checks == 'advisories' }}` e il commento "Prevent sudden announcement
of a new advisory from failing ci". I file sono piccoli: rustls/rustls `deny.toml` sono 18 righe,
tokio 21 e finisce con `[sources] unknown-registry = "deny"` / `unknown-git = "deny"`; atuinsh/atuin
aggiunge `allow-registry = ["https://github.com/rust-lang/crates.io-index"]` e mullvad restringe a
`[sources.allow-org] github = ["mullvad"]`. cargo-vet esiste in 1 repo su 35, tauri-apps/tauri
`.github/workflows/supply-chain.yml`, e il passo che dovrebbe bloccare e' commentato: gira solo
`cargo vet suggest`.

**Fuzzing.** 5 dei 25 repo Rust tengono target in-tree (ripgrep, ruff, image-rs, rustls, openssl in
C). La forma minima e' BurntSushi/ripgrep `fuzz/`: `Cargo.toml` di 26 righe con
`[package.metadata] cargo-fuzz = true`, `[workspace] members = ["."]` per restare fuori dal
workspace principale, e un solo target, `fuzz/fuzz_targets/fuzz_glob.rs`, non collegato alla CI.
Solo rustls esegue fuzzing sulle PR, via CIFuzz. 9 su 36 sono membri OSS-Fuzz. Nel vicinato di un
editor di sottotitoli, OSS-Fuzz copre libass, ffmpeg, freetype2, harfbuzz, dav1d, sqlite3.

**Lint Rust.** 20 su 25 repo Rust girano con `-D warnings`. 12 su 25 hanno una tabella `[lints]` o
`[workspace.lints]`, ma solo 4 dicono qualcosa su `unsafe_code` e **uno solo lo vieta**:
dani-garcia/vaultwarden, `Cargo.toml` di root, `[workspace.lints.rust] unsafe_code = "forbid"`,
`warnings = "deny"`, gruppi interi negati con `priority = -1`, poi `[workspace.lints.clippy]` con
`pedantic` a warn e il commento "Will be denied during CI!", e ogni eccezione con una riga di
motivo. astral-sh/uv e ruff si fermano a `unsafe_code = "warn"`, bevy a `"deny"`.

**TypeScript.** Su 547 workflow: zero occorrenze di eslint-plugin-security, semgrep, gitleaks,
trufflehog, osv-scanner, audit-ci. `npm audit`/`pnpm audit` in 2 su 36 (tauri, Signal-Desktop),
`dependency-review-action` in 1 su 36 (zed). Chi fa sul serio investe altrove: signalapp/Signal-Desktop
`.oxlintrc.json` con `"typeAware": true` e regole locali proprie da `./.oxlint/plugin.mjs`,
microsoft/vscode con `.eslint-plugin-local/` e circa 50 regole scritte a mano fra cui
`code-no-any-casts.ts` e `code-no-dangerous-type-assertions.ts`. Controesempio istruttivo:
bitwarden/clients `eslint.config.mjs:140` ha `"@typescript-eslint/no-explicit-any": "off", // TODO`.
Un password manager.

**Cooldown sulle release.** E' la pratica che si e' mossa piu' in fretta nell'ultimo anno. 17 dei 40
`dependabot.yml` hanno un blocco `cooldown:` e 14 delle 31 config Renovate hanno `minimumReleaseAge`,
quindi 31 config su 71. python/cpython `.github/dependabot.yml` porta il motivo inline:
`cooldown: default-days: 14` col commento "Cooldowns protect against supply chain attacks by
avoiding the highest-risk window immediately after new releases" (verificato leggendo il file oggi).
curl/curl usa la forma a scaglioni: `semver-major-days: 15`, `semver-minor-days: 7`,
`semver-patch-days: 3`. Anche grafana 14, syncthing 14, brew 7 con
`exclude: - Homebrew/actions/*`, vscode 7, pnpm 7, pip 7, node 5. Lato Renovate:
tauri-apps/tauri `renovate.json` `"minimumReleaseAge": "3 days"`, starship 4 giorni, rustup 3.
signalapp/Signal-Desktop lo impone al momento dell'installazione e non solo del bump:
`pnpm-workspace.yaml` con `minimumReleaseAge: 4320`, `minimumReleaseAgeStrict: true`,
`trustPolicy: no-downgrade`, `verifyStoreIntegrity: true`, `blockExoticSubdeps: true`.
Cronologia: `cooldown` GA il 2025-07-01, copertura completa degli ecosistemi il 2025-07-29, e dal
2026-07-14 GitHub applica 3 giorni di default a tutti gli aggiornamenti di versione. Quindi Sublore
oggi ha 3 giorni gratis e la domanda e' solo se ne vuole di piu'.

**Provenienza e firma dei binari.** 12 su 35 e 13 su 38 usano `actions/attest-build-provenance`.
L'esempio piu' piccolo e' BurntSushi/ripgrep `.github/workflows/release.yml`:
`permissions: contents: write, id-token: write, attestations: write`, poi
`actions/attest-build-provenance@977bb373ede98d70efdf65b84cb5f73e068dcc2a # v3.0.0` con
`subject-path: ${{ env.ASSET }}`. Anche FiloSottile/age, helix-editor/helix, syncthing, uv (che
attesta anche gli script di installazione `.sh` e `.ps1`), atuin, pnpm, obs-studio. Nessuno usa
slsa-github-generator. Dettaglio che vale piu' del conteggio: **su 12 che attestano, uno solo lo
documenta nel README** (sigstore/cosign). Gli altri producono l'attestazione e non dicono a nessuno
come verificarla.

Checksum accanto agli installer in 8 su 35, firme staccate in 6 su 35: mullvad spedisce un `.asc`
per ogni installer, syncthing `sha256sum.txt.asc` firmato in un job separato dove la chiave non
entra mai nei job di build, libsodium `.minisig` e `.sig`, spacedrive i `.sig` dell'updater Tauri
generati da `TAURI_SIGNING_PRIVATE_KEY` in `.github/workflows/release.yml`.

Authenticode su Windows senza dongle EV: 6 dei 16 progetti che spediscono installer firmano in CI,
con cinque meccanismi diversi. syncthing `azure/trusted-signing-action@0d74250c...`, deno
`Azure/artifact-signing-action@1d365fec...` con verifica `signtool verify /pa /v`, bitwarden
AzureSignTool 4.0.1 contro Key Vault, element-hq/element-desktop SSL.com eSigner CKA,
localsend SignPath via `signpath/github-action-submit-signing-request@v2` (piano gratuito open
source). **Nessuno mette un `.pfx` in un secret del repo.** element fa una cosa furba: esegue il
percorso di firma anche sulle build non di release, con credenziali demo, "per assicurarsi che
continui a funzionare". localsend invece ha i passi di firma commentati in `release.yml` con
"Signing temporarily disabled", che e' esattamente come muore un percorso di firma mai esercitato.

**SBOM.** 2 su 35 pubblicano un SBOM come asset di release: sigstore/cosign (SPDX per binario,
generato da goreleaser) e rustdesk (`syft dir:. -o cyclonedx-json=rustdesk.sbom.json` in
`.github/workflows/flutter-build.yml`). python/cpython ne genera uno per se', committato in
`Misc/sbom.spdx.json`, non per chi scarica. Tutti gli altri "hit" su una grep di SBOM erano header
`SPDX-License-Identifier`.

**Vendoring.** 1 su 35 ha una directory `vendor/` (atuin, e contiene due dipendenze git, non un
albero crates.io). age fa `go mod vendor` solo dentro il tarball sorgente, perche' i distributori
possano compilare offline. Nessun progetto Rust del campione committa l'output di `cargo vendor`.

### 2b. Visibile solo perche' i ruleset sono pubblici

121 ruleset letti come dati su 40 repo. 23 su 40 ne hanno almeno uno attivo; 14 su 40 sono protetti
ma solo con branch protection classica, quindi opachi dall'esterno e non verificabili nemmeno da
Scorecard, che su tauri-apps/tauri risponde `-1` con la motivazione testuale "some github tokens
can't read classic branch protection". 3 su 40 non hanno nessuna protezione sul branch di default:
torvalds/linux, vuejs/vue, RustCrypto/hashes.

La forma migliore da copiare per leggibilita' e' yt-dlp/yt-dlp, che spezza tutto in ruleset a scopo
singolo e con nomi che si spiegano da soli: `branch-all-01-no-force-push` (ref `~ALL`, regola
`non_fast_forward`), `branch-all-02-no-deletion`, `branch-all-03-no-creation`,
`branch-default-01-no-merge-commits`, `branch-default-02-require-approvals-(core-2)`,
`branch-default-03-require-passing-ci-(yt-dlp)`.

**Il numero che conta per chi lavora solo:** 6 su 40 usano la regola `pull_request` con
`required_approving_review_count: 0`. Sono zed-industries/zed (ruleset "Main", con merge queue),
syncthing/syncthing ("default branch"), rust-lang/crates.io ("main"), microsoft/TypeScript (il suo
ruleset di repo e' 0, l'1 arriva ereditato dall'organizzazione), pnpm/pnpm, electron/electron
("Require CODEOWNER Review"). 11 su 40 richiedono almeno un'approvazione, uno solo (yt-dlp) ne
richiede due.

Tag: 14 su 40 hanno almeno un ruleset sui tag; tutti e 14 bloccano la cancellazione, 12 limitano
anche la creazione. `GET /repos/{o}/{r}/tags/protection` ha risposto 404 su tutti e 40, quindi la
vecchia API di tag protection non esiste piu' e i ruleset sono l'unico meccanismo rimasto. Il
migliore da copiare e' astral-sh/uv `tags-are-immutable`: target `tag`, condizione
`ref_name.include: ["~ALL"]`, regole `deletion`, `non_fast_forward`, `update` (riletto e confermato
oggi: uv ha 6 ruleset, fra cui anche `tag-requires-release`, che pretende il passaggio per
l'environment `release`). ggml-org/whisper.cpp separa i canali: `releases-nightly` su `refs/tags/b*`
e `releases-official` su `refs/tags/v*` con in piu' i check di stato.

Release immutabili (`.immutable` sull'oggetto release): 9 su 35 (zed, mullvad, syncthing, pnpm, uv,
ruff, deno, brew, servo). Verificato oggi su uv: `true`.

Environment con revisore obbligatorio: 12 su 40 hanno almeno un environment con qualche regola, ma
solo 5 su 40 usano `required_reviewers` e solo 4 mettono `can_admins_bypass: false`. Il modello per
una persona sola e' pnpm/pnpm: environment `release` con un unico revisore richiesto, `zkochan`,
cioe' la stessa persona che preme merge, costretta a premere un secondo bottone separato prima che
qualcosa venga pubblicato. obs-studio usa l'environment `bouf` con cinque revisori nominali e
`can_admins_bypass: false`, pypa/pip `PyPI` con `Team:pip-committers`.

Firma dei commit come regola: 6 su 40 la impongono da qualche parte, solo 2 su tutto il branch di
default (electron, bun). node la applica in modo mirato ai branch di release e ai tag. Intanto 18 su
40 hanno tutti e 30 gli ultimi commit verificati senza nessuna regola che lo imponga, e 6 su 40 non
ne hanno nessuno verificato (linux, tensorflow, golang/go, openssl, ring, mpv).

CODEOWNERS: 18 su 40 hanno il file, ma solo 9 su 40 attivano davvero
`require_code_owner_review: true`. Meta' dei CODEOWNERS del campione sono quindi instradamento, non
imposizione.

SECURITY.md: 31 su 40 hanno una policy da qualche parte (24 nel repo, 7 solo a livello di
organizzazione, cosa che un account utente non puo' fare). E' un pavimento e non una misura: la
sonda cercava solo `SECURITY.md` e pyca/cryptography prende 10 al check Security-Policy di Scorecard
pur non avendo quel nome esatto. Private vulnerability reporting attivo su 16 su 36 e 19 su 40 nei
due campioni che l'hanno letto. Quattro repo (fra cui yt-dlp e zed) hanno PVR senza SECURITY.md:
il bottone e' tutta la loro policy.

### 2c. Non visibile, e come e' stato trattato

`security_and_analysis` e' admin-only su tutti e 40 i repo: secret scanning, push protection,
validity check e aggiornamenti di sicurezza Dependabot non sono misurabili dall'esterno. L'unica
inferenza disponibile e' negativa: **0 su 36 eseguono gitleaks o trufflehog in CI**, il che e'
indizio debole che si affidino allo scanner integrato di GitHub. Confidenza moderata, e non c'e' un
conteggio da dare.

Bypass actor nei ruleset: tutti vuoti, quindi redazione. Non si puo' dire chi puo' scavalcare cosa.

Se una PR di Dependabot venga letta da un umano prima del merge: invisibile per definizione.

Scorecard: 35 dei 40 hanno un record pubblico, 33 datati 2026 e 2 vecchissimi (vscode 2022-08-15,
curl 2022-11-09). Solo 4 su 40 eseguono il workflow in casa (ohmyzsh, node, TypeScript, electron).

## 3. Cosa dovrebbe fare Sublore, in ordine di effetto per ora spesa

Stato misurato di `xAlcahest/SubLore` il 2026-08-31, non stimato:

| Cosa                                            | Stato                                      |
| ----------------------------------------------- | ------------------------------------------ |
| `secret_scanning`                               | disabled                                   |
| `secret_scanning_push_protection`               | disabled                                   |
| `dependabot_security_updates`                   | disabled                                   |
| Private vulnerability reporting                 | `{"enabled": false}`                       |
| `branches/main` `protected`                     | `false`                                    |
| `/rulesets`                                     | `[]`                                       |
| SECURITY.md                                     | assente                                    |
| Release pubblicate                              | nessuna                                    |
| `permissions:` in `ci.yml`                      | presente, `contents: read` (riga 12)       |
| Action fissate per SHA                          | 0 su 11                                    |
| `unsafe` nel codice                             | 9 occorrenze, 4 file, tutti in `src-tauri` |
| Scorecard (misurato in container, v5.1.1, oggi) | 3.9                                        |

Il 3.9 di Scorecard va letto per parti, non come voto. I punti reali e a costo basso sono
Branch-Protection 0, Security-Policy 0, Pinned-Dependencies 0 e Vulnerabilities 0 (21 advisory aperti
sui grafi Cargo e npm: RUSTSEC-2024-0411..0420, 0429, 0370, RUSTSEC-2025-0075/0080/0081/0098/0100,
piu' quattro GHSA npm). Code-Review 0 e Contributors 0 sono strutturalmente irraggiungibili da soli
e vanno ignorati. Maintained 0 dice solo che il progetto ha meno di 90 giorni.

### Ordine di esecuzione

**A. Owner, UI di GitHub, dieci minuti. Oggi.**

1. Settings > Code security: attivare secret scanning e **push protection**. La push protection
   rifiuta la push invece di aprire un alert dopo, ed e' la versione meccanica della regola di
   CLAUDE.md §4 sui moduli chiusi, che oggi dipende da un agente che si ricorda.
2. Stessa pagina: attivare **Dependabot security updates** (oggi `disabled`). Senza questo, il file
   dependabot copre solo gli aggiornamenti di versione delle dipendenze dirette e un advisory su una
   crate transitiva non produce nulla.
3. Stessa pagina: attivare **private vulnerability reporting**. Il fallimento realistico su un repo
   appena pubblicato e' che qualcuno trovi un crash nel parser ASS, non abbia un canale privato e
   apra una issue pubblica con il file di prova allegato.
4. Controllare la tab Security per alert sulla storia gia' pubblicata. La push protection protegge
   il futuro. Qualunque cosa sia stata committata mentre il repo era privato e ora e' pubblica va
   trattata come trapelata e ruotata, non cancellata.

**B. Agente, un'ora, in PR.**

5. Pinnare per SHA le quattro action di terze parti in `.github/workflows/ci.yml`, con il commento
   di versione in coda: `dtolnay/rust-toolchain` (in due posti, job `check` e job `e2e`),
   `pnpm/action-setup@v6`, `Swatinem/rust-cache@v2`. `dtolnay/rust-toolchain@stable` e' la riga
   peggiore del repo: e' un branch, non un tag, quindi il codice che gira prima di ogni build puo'
   cambiare senza nessuna PR e nessun diff, e nessun bot puo' pinnarlo o aggiornarlo. Le tre
   `actions/*` (checkout, setup-node, upload-artifact) si possono lasciare sul tag: e' la posizione
   documentata di syncthing e helix, e microsoft/PowerToys scrive nero su bianco che l'hash pinning
   e' richiesto per le terze parti.
6. Aggiungere `persist-credentials: false` a tutti e tre i passi di checkout. Il checkout di default
   scrive il token del job in `.git/config`, dove ogni `build.rs` e ogni script di postinstall lo
   legge. Sublore compila un albero pieno di C (libmpv, ffmpeg, whisper) e installa
   `tauri-driver`: e' un percorso vivo, non teorico.
7. Aggiungere un job zizmor, copiando `dani-garcia/vaultwarden/.github/workflows/zizmor.yml` (31
   righe). E' l'unica cosa in questo elenco che continuera' a dire a un agente che ha scritto un
   workflow non sicuro quando l'owner avra' smesso di guardare. Da eseguire come job di CI e non come
   pre-commit, cosi' vale per qualunque cosa un agente spinga.
8. Sistemare `dependabot.yml` (sezione 5).
9. Aggiungere `actions/dependency-review-action` su `pull_request` (15 righe, nessun secret, funziona
   su repo pubblico), copiando `microsoft/PowerToys/.github/workflows/dependency-review.yml`.
   Avvertenza verificabile solo dopo: blocca il merge unicamente se il check e' marcato come
   richiesto nel ruleset, quindi va fatto dopo il punto C.
10. Aggiungere `deny.toml` sulla forma di rustls (18 righe): `[sources] unknown-registry = "deny"`,
    `unknown-git = "deny"`, allow-list delle licenze, e in CI la matrice di rust-lang/cargo con
    `continue-on-error` sulle sole advisory. Le licenze qui non sono stile: Sublore e' GPL-3.0 con un
    confine di moduli chiusi, e una dipendenza transitiva incompatibile e' un difetto legale.
11. `[lints]` per crate: `unsafe_code = "forbid"` sui cinque crate del workspace (sublore-formats,
    sublore-io, sublore-edit, sublore-project, sublore-asr), che oggi hanno zero `unsafe`, e
    `unsafe_code = "deny"` su `src-tauri` con `#[allow]` per blocco sui 9 punti FFI esistenti.
    Traccia una linea che un agente non puo' attraversare senza un diff evidente, esattamente dove
    entra l'input non fidato. Modello: vaultwarden.

**C. Owner, UI di GitHub, dieci minuti. Questa settimana.**

12. Ruleset 1, target branch, ref `~ALL`: blocca `non_fast_forward` e `deletion`. Non vieta niente
    di quello che l'owner fa gia', e toglie di mezzo la classe "un token rubato riscrive main e non
    resta traccia". Vale doppio con agenti che lavorano su worktree e branch usa e getta: un branch
    force-pushato si porta via il reflog e su GitHub non resta niente da recuperare.
13. Ruleset 2, target branch, `~DEFAULT_BRANCH`: `pull_request` con
    `required_approving_review_count: 0`, piu' `required_status_checks` sui job `check (ubuntu-latest)`,
    `check (windows-latest)` e `e2e smoke (ubuntu)`. Non aggiunge un revisore. Rende la CI un cancello
    invece di una cortesia, e impedisce a una macchina compromessa di spingere su main senza aprire
    qualcosa di visibile. **Non mettere 1**: GitHub non permette all'autore di approvare la propria
    PR.
14. Ruleset 3, target tag, `refs/tags/v*`: blocca `deletion`, `update`, `non_fast_forward`. `ci.yml`
    gia' si attiva su `tags: ["v*"]`, quindi il tag e' gia' il trigger di release e oggi puo' essere
    spostato o cancellato e ricreato. Modello: `astral-sh/uv/tags-are-immutable`.
15. Firma dei commit obbligatoria su `main`. Qui e' gratis: gli ultimi 30 commit sono gia' tutti
    `verified` con la chiave `0C8E5164EEED9AFB` e `commit.gpgsign` e' gia' `true` in locale. Va detto
    cosa difende davvero: non un portatile compromesso, visto che la chiave sta li'. Difende dal caso
    in cui un token GitHub rubato committi via API o via identita' bot e la cosa si confonda nella
    storia. Con un repo diretto da agenti, quello e' lo scenario piu' probabile dei due.

**D. Al momento della prima release, non prima.**

16. `actions/attest-build-provenance` nel workflow di release (tre righe piu' due permessi, nessuna
    chiave), sul modello di helix-editor/helix `release.yml`, che e' un binario Rust mantenuto da una
    persona. Piu' un file di checksum accanto agli installer.
17. **Quattro righe di README che dicono come verificarlo**, con il comando letterale
    `gh attestation verify Sublore_x.y.z_x64.msi --repo xAlcahest/SubLore`. E' il buco che lasciano
    tutti: 12 progetti su 35 producono l'attestazione, uno solo spiega come controllarla.
18. Environment `release` con l'owner come revisore richiesto e `can_admins_bypass: false`, sul
    modello pnpm. Ha senso solo quando esistera' una chiave di firma da proteggere; oggi sarebbe
    cerimonia, perche' non c'e' niente da custodire.
19. Firma Authenticode su Windows. Due strade realistiche per una persona: SignPath piano open source
    (Sublore e' GPL, qualifica) o Azure Trusted Signing. Da copiare il trucco di element-hq: eseguire
    il percorso di firma con credenziali demo su ogni build, cosi' non lo si esercita per la prima
    volta il giorno della release. Prerequisito del milestone Windows, non di M2.
20. Release immutabili: una spunta nelle impostazioni.

**E. Quando c'e' un pomeriggio, e ne vale la pena piu' di quanto sembri.**

21. Un crate `fuzz/` sulla forma di ripgrep: fuori dal workspace, un target per parser in
    sublore-formats, `cargo fuzz run` a mano prima di una release, e ogni crash committato in
    `fixtures/` come caso di regressione, che e' il ciclo che CLAUDE.md §5.3 chiede gia'. Sublore
    fa esattamente il mestiere per cui il fuzzing esiste: mangia file di provenienza arbitraria con
    CRLF, BOM, sovrapposizioni e righe malformate.
22. ESLint da `tseslint.configs.recommended` a `recommendedTypeChecked` con `projectService`. La
    configurazione attuale (`eslint.config.js`, tre righe di config) e' il livello non tipizzato:
    non vede oltre il confine di funzione e non puo' imporre la regola su `any` che CLAUDE.md §6
    chiede. Il pezzo che serve davvero e' `no-floating-promises`, cioe' i percorsi async non gestiti
    al confine IPC di Tauri. Costo: una giornata di pulizia.
23. CodeQL default setup (una spunta, nessuno YAML): copre Rust in `build-mode: none`, quindi non
    puo' rompere la CI di libmpv, piu' TypeScript e Actions. Onesta' su cosa rende: il pacchetto
    Rust e' giovane, e su un'app locale senza superficie di rete il valore sta nelle query su
    Actions e TypeScript, non su Rust.

## 4. Cosa saltare, e perche'

**Blocco dell'egress sul runner (step-security/harden-runner).** 3 su 38 lo usano, e **0 su 38
bloccano davvero**: tutte e tre le occorrenze sono `egress-policy: audit`. ossf/scorecard e
nodejs/node hanno lo stesso identico commento, `# TODO: change to 'egress-policy: block' after
couple of runs`, e ossf/scorecard e' il repo dell'organizzazione che pubblica le linee guida, fermo
su quel TODO in tutti e 10 i suoi workflow. Per Sublore significherebbe mantenere una allow-list per
crates.io, static.crates.io, il registry npm, GitHub, il CDN della toolchain, i mirror apt di
WebKitGTK e i download della toolchain Windows, con la build rotta a ogni nuova dipendenza.

**Generare i workflow da codice.** 2 su 38 (zed con `cargo xtask workflows`, deno con i file
`*.generated.yml`). E' davvero il motivo per cui i numeri di zed sono perfetti, ed e' tentante perche'
Sublore e' Rust e il pattern e' idiomatico. Sarebbe un generatore da mantenere per governare quattro
o cinque file YAML. zizmor in CI da' il 90% della stessa garanzia. Sta qui scritto perche' quando un
agente lo proporra' citando zed, ci sia una risposta con un motivo.

**OSS-Fuzz.** 9 su 36 sono membri, 1 solo esegue CIFuzz sulle PR. Richiede rilevanza pubblica
dimostrabile, un'integrazione di build mantenuta dentro google/oss-fuzz e un contatto di sicurezza
che risponda a scadenze di divulgazione a 90 giorni. E' un obbligo permanente su una persona sola.

**cargo-vet.** 1 su 35, ed e' tauri, dove il passo bloccante e' commentato con "Enable this again to
break the workflow once we have a reasonable amount of suggestions". Il progetto che ne avrebbe piu'
bisogno nel campione non e' riuscito a farlo attecchire.

**SBOM.** 2 su 35 lo pubblicano. Non compra niente finche' un ufficio acquisti non lo chiede. Se un
giorno lo chiede, e' un passo `syft` nel workflow di release, come fa rustdesk. Non impalcarlo ora.

**Vendoring delle dipendenze.** 1 su 35. Scambia un rischio gia' coperto da lockfile committato piu'
cooldown con una tassa di manutenzione permanente e un diff enorme dentro cui un agente puo'
nascondere qualsiasi cosa.

**CODEOWNERS per imporre le review.** 18 su 40 hanno il file ma solo 9 lo rendono vincolante. Con un
solo umano, `codeowners: true` piu' una approvazione richiesta e' uno stallo, e `codeowners: true`
con 0 approvazioni e' un no-op che instrada la richiesta a chi l'ha aperta. L'unica variante che
guadagnera' il suo posto e' quella di electron ("Limit CODEOWNERS & .github to gatekeepers"): una
regola per percorso su `.github/workflows/**` e sull'interfaccia dei moduli, quando esistera' il
confine chiuso.

**Auto-merge delle PR di Dependabot.** 2 su 95 hanno un workflow dedicato (prometheus, ruby). Qui e'
da escludere per una ragione specifica del repo: `ci.yml` ha `continue-on-error: true` su sette passi
E2E su otto, e il verdetto vero e' un `grep` sui log nel passo finale. L'auto-merge vale quanto i
check su cui aspetta, e questi hanno come segnale di pass una regex su testo di log.

**Badge Scorecard e workflow Scorecard.** 4 su 40 lo eseguono in casa. Su un progetto solo,
Code-Review e Contributors sono 0 per costruzione e il badge pubblicizza un numero che non si puo'
muovere. Eseguirlo una volta da container come checklist, senza badge.

**`reviewers:` e `assignees:` in dependabot.yml.** 1 su 40 e 0 su 40. GitHub le ha deprecate.

**eslint-plugin-security.** 0 su 36. Le sue regole guardano pattern Node lato server (child_process,
path fs non letterali, euristiche su regex DoS) e in una webview sparerebbero rumore su niente.

**Egress, SLSA generator, cosign per i binari desktop.** 0 su 35 usa slsa-github-generator e nessuno
firma binari desktop con cosign. L'attestazione nativa di GitHub e' quello che fa il campione.

## 5. Il `dependabot.yml` di Sublore, voce per voce

Il file attuale e' migliore di 30 dei 36 repo aperti in uno dei campioni: tre ecosistemi coperti,
raggruppamento su `minor, patch` che e' esattamente la forma di helix-editor/helix e actions/checkout,
`commit-message: prefix` con i prefissi conventional-commit giusti (6 su 40 si prendono la briga), e
un limite di 5 PR che sta nella fascia bassa e sana, accanto a systemd. Quello che manca:

**1. `ignore` su libmpv2 non qualificato. Questo e' un bug, non una raffinatezza.** La documentazione
GitHub dice che le condizioni `ignore` si applicano anche agli aggiornamenti di sicurezza, quindi
com'e' scritto ora un advisory pubblicato contro libmpv2 non produrrebbe **nessuna** PR, in silenzio.
Va contro il motivo per cui il pin esiste, che e' l'accoppiamento con la DLL. Il campione qualifica
sempre: sharkdp/bat blocca la singola versione (`versions: - 0.13.17`), cryptomator un intervallo,
nushell e dotnet/runtime lo scopo per `update-types`. 7 dei 10 file che usano `ignore` lo qualificano.

```yaml
ignore:
  # Pinned deliberately and verified against the DLL it links: see the libmpv step in ci.yml.
  # Scoped so a published advisory still opens a PR.
  - dependency-name: libmpv2
    update-types:
      - version-update:semver-major
      - version-update:semver-minor
```

**2. Nessun `cooldown`.** Da aggiungere in coda a tutte e tre le voci, forma a scaglioni di curl:

```yaml
cooldown:
  semver-major-days: 15
  semver-minor-days: 7
  semver-patch-days: 3
```

Il default di GitHub da luglio 2026 e' 3 giorni per tutto, il che non distingue una patch di una
crate gia' fidata da un major nuovo di zecca di un pacchetto npm transitivo. Non si applica agli
aggiornamenti di sicurezza, per progetto, quindi non costa niente in tempo di risposta a una CVE.

**3. Gli aggiornamenti di sicurezza Dependabot sono spenti.** Non e' una riga del file, e' la spunta
in Settings, misurata oggi come `disabled`. Senza, la voce cargo copre solo le dipendenze dirette e
una crate transitiva vulnerabile non compare mai nella PR settimanale. E' l'alternativa giusta a
`allow: - dependency-type: all`, che su un workspace Cargo produce troppe PR per una persona sola (6
file su 40 usano `allow`, e solo 3 lo aprono a `all`: pyca/cryptography, RustCrypto/hashes, brew).

**4. Il package manager non e' fissato con l'hash.** `package.json` ha
`"packageManager": "pnpm@10.28.2"`. element-hq/element-desktop scrive
`"pnpm@10.32.1+sha512.a706938f0e89ac1456b6563eab4edf1d1faf3368d1191fc5c59790e96dc918e4456ab2e67d613de1043d2e8c81f87303e6b40d4ffeca9df15ef1ad567348f2be"`,
cosi' corepack verifica il binario che scarica, anche sul runner Windows. `onlyBuiltDependencies` c'e'
gia', ed e' la meta' che conta di piu'. Da valutare anche `minimumReleaseAge: 4320` in un
`pnpm-workspace.yaml` (Signal, pnpm stesso): la chiave esiste da pnpm 10.16.0, settembre 2025, e
Sublore e' su 10.28.2. `blockExoticSubdeps` e `trustPolicy` di Signal richiedono un pnpm piu' nuovo.

**5. `--locked` manca sulle build.** `pnpm install --frozen-lockfile` c'e', e
`cargo install tauri-driver --version 2.0.6 --locked` anche, ma `cargo build` e `cargo test` no. Con
`--locked` un lockfile stantio o modificato a mano fa fallire la CI invece di essere aggiornato in
silenzio. E' quello che fanno uv, ruff, atuin, bitwarden, deno, element.

**6. Cinque pin esterni che nessuno sorveglia.** Non e' un buco di dependabot, e' un buco che
dependabot non puo' colmare per costruzione: non ha i custom manager di Renovate.

| Pin                                                   | Dove                     | Analogo nel campione                                                        |
| ----------------------------------------------------- | ------------------------ | --------------------------------------------------------------------------- |
| `commit=978113305b...` (whisper.cpp)                  | `whisper.pin`            | rust-lang/crates.io, `datasourceTemplate: 'git-refs'` su SHA a 40 caratteri |
| `$tag = "20260814"` + `$sha = "0af22b28..."` (libmpv) | `ci.yml`, passo Windows  | astral-sh/uv, regex che tiene insieme versione e SHA-256                    |
| `tauri-driver --version 2.0.6`                        | `ci.yml`, job e2e        | starship, regex su `cargo install .* --version` nei workflow                |
| `channel = "1.93.0"`                                  | `rust-toolchain.toml`    | astral-sh/uv, custom manager sul canale contro github-releases              |
| `sha256=921e4cf8...` (modello whisper)                | `scripts/fetch-model.sh` | stesso caso uv                                                              |

4 delle 31 config Renovate definiscono `customManagers`. Migrare a Renovate per questo non conviene
oggi. L'alternativa da poche ore: un workflow schedulato che fallisce quando uno dei cinque pin e'
indietro rispetto a monte. Attenzione al modo in cui fallisce: un job notturno che fallisce in
silenzio per un manutentore solo e' peggio di nessun job, perche' fabbrica la sensazione di
copertura. astral-sh/ruff `.github/workflows/daily_fuzz.yaml` risolve cosi': un secondo job
`create-issue-on-failure` che apre una issue quando il primo fallisce su schedule. Da copiare quella
forma per qualunque cosa schedulata.

**7. Cose che il file fa bene e vanno lasciate stare.** Il raggruppamento minor+patch senza
`patterns` e' la forma esatta di helix. `open-pull-requests-limit: 5` e' giusto. Niente `reviewers`
e' giusto. Niente `directories` plurale e' giusto: un solo `Cargo.lock`, un solo `package.json`, un
solo branch. Il commento sul fatto che rust-toolchain.toml si bumpa a mano resta valido, e non e' in
conflitto con l'idea del punto 6: un watcher aprirebbe la PR, l'owner la mergia comunque da solo.

`open-pull-requests-limit: 0` (hashicorp/terraform, con il commento "Disable regular version updates
and only use Dependabot for security updates") non serve adesso, ma e' esattamente come si fara'
girare il repo privato dei moduli chiusi: solo aggiornamenti di sicurezza, nessun flusso settimanale
da leggere.

## Cosa resta non verificato

- Le impostazioni `security_and_analysis` degli altri 40 repo: admin-only. L'unico indizio e' che 0
  su 36 girano gitleaks o trufflehog in CI, confidenza moderata.
- I bypass actor di tutti e 121 i ruleset letti: array vuoto, quindi redatti.
- Chi legge davvero una PR di Dependabot prima del merge, in qualunque repo.
- Il conteggio di `dependency-review-action`: 1 su 95 per scansione dei nomi dei workflow e 1 su 36
  in un altro campione. Chi lo esegue dentro un workflow esistente non comparirebbe. Trattare come
  pavimento, non come misura.
- Il conteggio SECURITY.md (31 su 40) e' un pavimento: la sonda cercava solo quel nome esatto, e
  almeno un miss (pyca/cryptography) e' un artefatto di estensione.
- Che il blocco di merge di `dependency-review-action` funzioni davvero: dipende dal check marcato
  come richiesto nel ruleset, cosa che oggi su Sublore non esiste ancora.
- Che `ignore` senza qualificatori sopprima gli aggiornamenti di sicurezza: e' documentato da
  GitHub, non e' stato riprodotto sperimentalmente su questo repo.
- Ogni verdetto comportamentale su Windows: qui non se ne danno, e nessuna di queste misure e' stata
  esercitata su un runner Windows di Sublore.
