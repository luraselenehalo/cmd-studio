# G14, G15, G18 — research

- **Date fetched:** 2026-10-10.
- **Method:**
  - The GitHub license API (`GET /repos/{owner}/{repo}/license`), which gives the SPDX id and the LICENSE text. The copyright line below is copied from that text.
  - The raw README files.
  - The pages linked below.
- **Wording:** conditions are summarized in my own words, with links. Two kinds of text are copied exactly: the attribution SignPath requires us to show, and the copyright notices we must keep.

## G14 — licenses of upstream color schemes and fonts: ✅ (two source choices)

| What | Repo (LICENSE link) | SPDX (GitHub API) | Copyright line in LICENSE | Note |
|---|---|---|---|---|
| Tokyo Night (VS Code) | [tokyo-night/tokyo-night-vscode-theme](https://github.com/tokyo-night/tokyo-night-vscode-theme/blob/master/LICENSE.txt) | MIT | Copyright (c) 2018-present Enkia | **use this** (formerly enkia/…) |
| Tokyo Night (Neovim) | [folke/tokyonight.nvim](https://github.com/folke/tokyonight.nvim/blob/main/LICENSE) | Apache-2.0 | none in LICENSE | not used |
| Catppuccin | [catppuccin/catppuccin](https://github.com/catppuccin/catppuccin/blob/main/LICENSE) | MIT | Copyright (c) 2021 Catppuccin | |
| Dracula | [dracula/dracula-theme](https://github.com/dracula/dracula-theme/blob/main/LICENSE) | MIT | Copyright (c) 2023 Dracula Theme | |
| Nord | [nordtheme/nord](https://github.com/nordtheme/nord/blob/develop/license) | MIT | Copyright (c) 2016-present Sven Greb | |
| Gruvbox (original) | morhetz/gruvbox | API 404: no LICENSE file | — | README says MIT/X11 but has no license text |
| Gruvbox (community) | [gruvbox-community/gruvbox](https://github.com/gruvbox-community/gruvbox/blob/master/LICENSE.md) | MIT | Copyright (c) 2018 Pavel Pertsev | **use this**; Pavel Pertsev is the original author |
| One Dark | [atom/one-dark-syntax](https://github.com/atom/one-dark-syntax/blob/master/LICENSE.md) | MIT | Copyright (c) 2016 GitHub Inc. | |
| Solarized | [altercation/solarized](https://github.com/altercation/solarized/blob/master/LICENSE) | MIT | Copyright (c) 2011 Ethan Schoonover | |
| Cascadia Code | [microsoft/cascadia-code](https://github.com/microsoft/cascadia-code/blob/main/LICENSE) | NOASSERTION | Copyright (c) 2019 - Present, Microsoft Corporation | SIL OFL 1.1 text, with the reserved font name "Cascadia Code" |
| Nerd Fonts | [ryanoasis/nerd-fonts](https://github.com/ryanoasis/nerd-fonts/blob/master/LICENSE) | NOASSERTION | Copyright (c) 2014 Ryan L McIntyre | patched fonts under SIL OFL 1.1; the patcher and scripts under MIT |
| Oh My Posh | [JanDeDobbeleer/oh-my-posh](https://github.com/JanDeDobbeleer/oh-my-posh/blob/main/COPYING) | MIT | Copyright 2022 Jan De Dobbeleer | |
| Clink | [chrisant996/clink](https://github.com/chrisant996/clink/blob/master/LICENSE) | GPL-3.0 | none: the LICENSE file is the plain GPL text | README: Clink is distributed under GPL v3.0; it includes GNU Readline. Source headers carry the notices, e.g. `Copyright (c) 2012 Martin Ridgers` in `inject.cpp` |

All of these licenses are OSI-approved: MIT, Apache-2.0, GPL-3.0 and OFL-1.1. SignPath requires this for every component (see G15).

## G15 — SignPath Foundation conditions: ✅, with one new risk

Source: [signpath.org/terms](https://signpath.org/terms). The page shows no "last updated" date. It calls its code of conduct a draft.

### Conditions, summarized

| Area | Condition |
|---|---|
| License | Every component must use an OSI-approved license, with no commercial dual licensing. |
| Team | Whoever signs must also be the people who develop and maintain the project. |
| Build | Binaries must be built from source in a verifiable way. A signature states that the binary is an automated build of that source. |
| Release | The project must already be released, in the form that will be signed. Every release needs a manual approval before it is signed. |
| Policy | The project's home page must state a code signing policy. |
| Publisher | SignPath Foundation appears as the publisher of the signed software. |
| Attribution | Show this text exactly: "Free code signing provided by SignPath.io, certificate by SignPath Foundation" |

### The application form

[signpath.org/apply](https://signpath.org/apply) is an embedded HubSpot form. It renders only in a browser, and I read it there without filling anything in.

| Kind | What the form asks |
|---|---|
| Required | Project name, repository URL (GitHub or GitLab), homepage URL, tagline, category, description, **reputation**, build system, first and last name, email, and how you found SignPath |
| Optional | Download page URL, privacy policy URL, Wikipedia URL, maintainer type, company |
| Download page | If given, it must say that the project uses SignPath Foundation for code signing |
| Privacy policy | Needed only if the software collects user data. CMD Studio has no telemetry. |
| Required checkboxes | Agree to the code of conduct, accepting that certificates are issued in SignPath Foundation's name and can be revoked if the terms are broken; and consent to SignPath storing personal data |

### For the plan

- **Founder step per release:** each release needs a manual approval in SignPath, so the release pipeline in spec §6.4 has a step for the founder.
- **New risk: reputation.** The form requires evidence that the project is widely used or trusted, such as press, blog posts, download numbers or community threads, and the terms require an existing release.
  - A brand-new CMD Studio may be turned down at first.
  - The first public releases may have to ship unsigned, with SmartScreen warnings, until the project has a track record.
  - This is the founder's decision. Task 9 adds it to spec §6.

## G18 — Windows Terminal requirements and Store link: ✅

- **Minimum Windows:** the [microsoft/terminal README](https://github.com/microsoft/terminal) says Windows Terminal needs Windows 10 version 2004 (build 19041) or later. It says this in two places: the install section and the build section, as build 10.0.19041.0.
- **Store page:** [apps.microsoft.com/detail/9N0DX20HK701](https://apps.microsoft.com/detail/9N0DX20HK701) has the page title "Windows Terminal" (Microsoft Store). So the deep link `ms-windows-store://pdp/?ProductId=9N0DX20HK701` is the right product. The README links to the Store through `https://aka.ms/terminal`.
- **For the spec:** the minimum supported Windows 10 is version 2004 (build 19041).
