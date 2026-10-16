import type { en } from "./en";

/** Every key of the English dictionary; the Thai one must have exactly these. */
export type Key = keyof typeof en;
export type Dictionary = Record<Key, string>;
export type Language = "en" | "th";
