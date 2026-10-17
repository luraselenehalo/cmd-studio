// The twelve preset files of the repository, as the mock's library. Vite reads them as text
// (`?raw`); the real app never loads them this way (the Rust side embeds them). Only the mock
// and the tests import this module, and no production module imports the mock.
import type { Theme } from "../bindings/Theme";
import terracotta from "../../../../presets/terracotta.cmdtheme?raw";
import tokyoNight from "../../../../presets/tokyo-night.cmdtheme?raw";
import catppuccinMocha from "../../../../presets/catppuccin-mocha.cmdtheme?raw";
import dracula from "../../../../presets/dracula.cmdtheme?raw";
import nord from "../../../../presets/nord.cmdtheme?raw";
import gruvbox from "../../../../presets/gruvbox.cmdtheme?raw";
import oneDark from "../../../../presets/one-dark.cmdtheme?raw";
import retroGreen from "../../../../presets/retro-green.cmdtheme?raw";
import catppuccinLatte from "../../../../presets/catppuccin-latte.cmdtheme?raw";
import solarizedLight from "../../../../presets/solarized-light.cmdtheme?raw";
import daylight from "../../../../presets/daylight.cmdtheme?raw";
import paper from "../../../../presets/paper.cmdtheme?raw";

const SOURCES = [
  terracotta,
  tokyoNight,
  catppuccinMocha,
  dracula,
  nord,
  gruvbox,
  oneDark,
  retroGreen,
  catppuccinLatte,
  solarizedLight,
  daylight,
  paper,
];

export const PRESETS: Theme[] = SOURCES.map((text) => JSON.parse(text) as Theme);
