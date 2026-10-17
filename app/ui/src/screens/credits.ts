// The credits page's data (spec §6.6), from docs/credits.md: the bundled programs and font,
// and the color schemes the built-in themes use, each with its license and copyright line.
export interface Credit {
  name: string;
  version?: string;
  license: string;
  copyright: string;
  source: string;
}

export const BUNDLED: Credit[] = [
  {
    name: "Clink",
    version: "1.9.34",
    license: "GPL-3.0",
    copyright: "Martin Ridgers and contributors",
    source: "https://github.com/chrisant996/clink/tree/v1.9.34",
  },
  {
    name: "Oh My Posh",
    version: "31.5.0",
    license: "MIT",
    copyright: "Copyright 2022 Jan De Dobbeleer",
    source: "https://github.com/JanDeDobbeleer/oh-my-posh",
  },
  {
    name: "CaskaydiaCove Nerd Font",
    version: "Nerd Fonts 3.5.1",
    license: "SIL OFL 1.1",
    copyright:
      "Copyright (c) 2019 - Present, Microsoft Corporation (Cascadia Code); Copyright (c) 2014 Ryan L McIntyre (Nerd Fonts)",
    source: "https://github.com/ryanoasis/nerd-fonts",
  },
];

export const SCHEMES: Credit[] = [
  {
    name: "Tokyo Night",
    license: "MIT",
    copyright: "Copyright (c) 2018-present Enkia",
    source: "https://github.com/tokyo-night/tokyo-night-vscode-theme",
  },
  {
    name: "Catppuccin Mocha, Catppuccin Latte",
    license: "MIT",
    copyright: "Copyright (c) 2021 Catppuccin",
    source: "https://github.com/catppuccin/windows-terminal",
  },
  {
    name: "Dracula",
    license: "MIT",
    copyright: "Copyright (c) 2019 thismat (the Windows Terminal scheme); Copyright (c) 2023 Dracula Theme (the palette's names)",
    source: "https://github.com/dracula/windows-terminal",
  },
  {
    name: "Nord",
    license: "MIT",
    copyright: "Copyright (c) 2016-present Sven Greb <development@svengreb.de> (https://www.svengreb.de)",
    source: "https://github.com/nordtheme/alacritty",
  },
  {
    name: "Gruvbox",
    license: "MIT",
    copyright: "Copyright (c) 2018 Pavel Pertsev",
    source: "https://github.com/gruvbox-community/gruvbox",
  },
  {
    name: "One Dark",
    license: "MIT",
    copyright: "Copyright (c) 2016 GitHub Inc.",
    source: "https://github.com/atom/one-dark-syntax",
  },
  {
    name: "Solarized Light",
    license: "MIT",
    copyright: "Copyright (c) 2011 Ethan Schoonover",
    source: "https://github.com/altercation/solarized",
  },
  {
    name: "Terracotta, Retro green, Daylight, Paper",
    license: "MIT",
    copyright: "CMD Studio contributors",
    source: "https://github.com/luraselenehalo/cmd-studio",
  },
];
