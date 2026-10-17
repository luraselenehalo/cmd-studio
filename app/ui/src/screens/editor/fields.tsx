// The editor's number fields: whole numbers only, typed freely.
import { useState } from "react";

/**
 * A whole number between `min` and `max` (ruling: the font size and the padding are `u8` and the
 * width `u16` on the Rust side, and a fraction fails Tauri's argument check, so the preview and
 * the draft would silently stop). NaN gives `min`.
 */
export function clamp(n: number, min: number, max: number): number {
  if (!Number.isFinite(n)) return min;
  return Math.min(max, Math.max(min, Math.round(n)));
}

/**
 * A number field that lets the user type freely: what is typed is kept as text, and each text
 * that is a number in range goes on, rounded, at once; on leaving the field it is clamped (or,
 * when empty, the value comes back).
 */
export function NumberField({
  label,
  value,
  min,
  max,
  onChange,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  onChange: (n: number) => void;
}) {
  const [text, setText] = useState<string | null>(null);
  return (
    <label>
      {label}
      <input
        type="number"
        min={min}
        max={max}
        step={1}
        value={text ?? String(value)}
        onChange={(e) => {
          const raw = e.target.value;
          setText(raw);
          const n = Number(raw);
          if (raw.trim() !== "" && Number.isFinite(n) && n >= min && n <= max) {
            const whole = clamp(n, min, max);
            if (whole !== value) onChange(whole);
          }
        }}
        onBlur={() => {
          if (text !== null && text.trim() !== "") {
            const whole = clamp(Number(text), min, max);
            if (whole !== value) onChange(whole);
          }
          setText(null);
        }}
      />
    </label>
  );
}
