# Credits and licenses

CMD Studio is MIT-licensed. It bundles or derives from the components below, each under its own license. The app's credits page (spec §6.6) shows this list. Licenses and copyright lines were checked at the source on 2026-10-10; see `docs/spikes/research.md`.

## Bundled programs and fonts

| Component | Version | License | Copyright | Source |
|---|---|---|---|---|
| Clink | 1.9.34 (bundled unmodified) | GPL-3.0 | Martin Ridgers and contributors (notices in the source headers) | https://github.com/chrisant996/clink/tree/v1.9.34 |
| Oh My Posh | 31.5.0 (bundled unmodified) | MIT | Copyright 2022 Jan De Dobbeleer | https://github.com/JanDeDobbeleer/oh-my-posh |
| CaskaydiaCove Nerd Font | pinned when bundled; the G12 probe used v3.5.1 | SIL OFL 1.1 | Copyright (c) 2019 - Present, Microsoft Corporation (Cascadia Code); Copyright (c) 2014 Ryan L McIntyre (Nerd Fonts) | https://github.com/ryanoasis/nerd-fonts |

CaskaydiaCove is Nerd Fonts' patched build of Cascadia Code. Cascadia Code's license reserves the font name "Cascadia Code", which is why the patched font has another name. Windows (GDI+) reports its family as `CaskaydiaCove NF`. The installer must include the OFL text.

## Color schemes used by presets

| Preset | Colors from | License | Copyright |
|---|---|---|---|
| Tokyo Night | https://github.com/tokyo-night/tokyo-night-vscode-theme | MIT | Copyright (c) 2018-present Enkia |
| Catppuccin Mocha, Catppuccin Latte | https://github.com/catppuccin/catppuccin | MIT | Copyright (c) 2021 Catppuccin |
| Dracula | https://github.com/dracula/dracula-theme | MIT | Copyright (c) 2023 Dracula Theme |
| Nord | https://github.com/nordtheme/nord | MIT | Copyright (c) 2016-present Sven Greb |
| Gruvbox | https://github.com/gruvbox-community/gruvbox | MIT | Copyright (c) 2018 Pavel Pertsev |
| One Dark | https://github.com/atom/one-dark-syntax | MIT | Copyright (c) 2016 GitHub Inc. |
| Solarized Light | https://github.com/altercation/solarized | MIT | Copyright (c) 2011 Ethan Schoonover |
| Terracotta, Retro green, Daylight, Paper | CMD Studio | MIT | CMD Studio contributors |

Take Tokyo Night's colors from the MIT-licensed VS Code theme, not from `folke/tokyonight.nvim`, which is Apache-2.0. Take Gruvbox's colors from gruvbox-community, because the original `morhetz/gruvbox` has no license file.

## Code signing

When signing through SignPath Foundation is set up, show this attribution exactly as SignPath requires: "Free code signing provided by SignPath.io, certificate by SignPath Foundation".
