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
| Catppuccin Mocha, Catppuccin Latte | https://github.com/catppuccin/windows-terminal (color names: https://github.com/catppuccin/palette) | MIT | Copyright (c) 2021 Catppuccin |
| Dracula | https://github.com/dracula/windows-terminal (color names: https://github.com/dracula/dracula-theme) | MIT | Copyright (c) 2019 thismat (the Windows Terminal scheme); Copyright (c) 2023 Dracula Theme (the palette's names) |
| Nord | https://github.com/nordtheme/alacritty | MIT | Copyright (c) 2016-present Sven Greb <development@svengreb.de> (https://www.svengreb.de) |
| Gruvbox | https://github.com/gruvbox-community/gruvbox | MIT | Copyright (c) 2018 Pavel Pertsev |
| One Dark | https://github.com/atom/one-dark-syntax | MIT | Copyright (c) 2016 GitHub Inc. |
| Solarized Light | https://github.com/altercation/solarized | MIT | Copyright (c) 2011 Ethan Schoonover |
| Terracotta, Retro green, Daylight, Paper | CMD Studio | MIT | CMD Studio contributors |

Take Tokyo Night's colors from the MIT-licensed VS Code theme, not from `folke/tokyonight.nvim`, which is Apache-2.0. Take Gruvbox's colors from gruvbox-community, because the original `morhetz/gruvbox` has no license file. Nord has no Windows Terminal scheme of its own; its Alacritty port gives the terminal colors.

The values were taken on 2026-10-11 from these commits:

| Preset | Repository at commit | File |
|---|---|---|
| Tokyo Night | `tokyo-night/tokyo-night-vscode-theme@7c0f11eaef322f293621ca7befe462214b7ea468` | `themes/tokyo-night-color-theme.json` |
| Catppuccin Mocha, Latte | `catppuccin/windows-terminal@4d8bb2f00fb86927a98dd3502cdec74a76d25d7b` | `mocha.json`, `latte.json` |
| Dracula | `dracula/windows-terminal@06f63714a0477a7b5c12cf0aa94ecf92d78f32ca` | `dracula.json` |
| Nord | `nordtheme/alacritty@9949642f3903e8fcb62bfc03f09410e3d78440c2` | `src/nord.yaml` |
| Gruvbox | `gruvbox-community/gruvbox@180ad85971343df68be3422a5630fa84e45a9ab2` | `colors/gruvbox.vim` |
| One Dark | `atom/one-dark-syntax@9c96f4454362267ac45322063e193ccf9d2debb1` | `styles/colors.less`, `styles/syntax-variables.less` |
| Solarized Light | `altercation/solarized@62f656a02f93c5190a8753159e34b385588d5ff3` | `README.md` ("The Values") |

Where a project has no color for one of a theme's slots, CMD Studio took one of the project's own colors: the accent, the muted text and the frame of every upstream preset, Tokyo Night's background and text (its editor's, D1), and One Dark's terminal colors (One Dark has none; the hex values are One Dark's, which goes in which ANSI slot is CMD Studio's). The design spec's appendix B lists these choices.

## Code signing

When signing through SignPath Foundation is set up, show this attribution exactly as SignPath requires: "Free code signing provided by SignPath.io, certificate by SignPath Foundation".
