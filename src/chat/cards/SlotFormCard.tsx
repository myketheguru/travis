/**
 * SlotFormCard — v0.28.30 Phase C.
 *
 * Mini form for workflow slot-filling. Each field has a type (text,
 * longtext, number, currency, date, select, checkbox). Submit
 * collects values as JSON and sends to Travis as a structured next
 * turn. Turns the workflow-led interaction memory into a real touchable
 * surface.
 */
import { useState } from "react";
import { useAppStore } from "../../stores/app";
import type { SlotField } from "../../lib/richResponse";

interface Props {
  title?: string;
  intro?: string;
  fields: SlotField[];
  submit_label?: string;
  submit_verb?: string;
  narration?: string;
}

export function SlotFormCard({ title, intro, fields, submit_label, submit_verb, narration }: Props) {
  const setPendingComposerSubmit = useAppStore((s) => s.setPendingComposerSubmit);
  const [state, setState] = useState<Record<string, string | number | boolean>>(() => {
    const initial: Record<string, string | number | boolean> = {};
    for (const f of fields) if (f.value !== undefined) initial[f.key] = f.value;
    return initial;
  });
  const [errors, setErrors] = useState<Record<string, string>>({});

  const setValue = (key: string, v: string | number | boolean) => {
    setState((prev) => ({ ...prev, [key]: v }));
    if (errors[key]) setErrors((prev) => { const n = { ...prev }; delete n[key]; return n; });
  };

  const doSubmit = () => {
    const missing: Record<string, string> = {};
    for (const f of fields) {
      if (f.required && (state[f.key] === undefined || state[f.key] === "")) {
        missing[f.key] = `${f.label} is required`;
      }
    }
    if (Object.keys(missing).length > 0) { setErrors(missing); return; }
    if (submit_verb) {
      setPendingComposerSubmit(`${submit_verb} ${JSON.stringify(state)}`);
    } else {
      const parts = Object.entries(state).map(([k, v]) => `${k}=${v}`).join(", ");
      setPendingComposerSubmit(`Submit: ${parts}`);
    }
  };

  return (
    <div
      className="rounded-2xl overflow-hidden"
      style={{
        border: "1px solid rgba(189, 158, 255, 0.32)",
        background: "linear-gradient(180deg, rgba(28, 24, 40, 0.62), rgba(20, 18, 30, 0.58))",
      }}
    >
      {(title || intro) && (
        <div className="px-4 pt-3 pb-2" style={{ borderBottom: "1px solid rgba(189, 158, 255, 0.18)" }}>
          {title && (
            <div className="text-[10.5px] uppercase tracking-[0.22em] font-mono mb-1" style={{ color: "rgba(189, 158, 255, 0.85)" }}>{title}</div>
          )}
          {intro && (
            <div className="text-[13px] leading-snug" style={{ color: "rgba(236, 236, 241, 0.88)" }}>{intro}</div>
          )}
        </div>
      )}
      <div className="px-4 py-3 flex flex-col gap-3">
        {fields.map((f) => (
          <div key={f.key}>
            <label className="text-[10.5px] uppercase tracking-wider font-mono block mb-1" style={{ color: "rgba(236, 236, 241, 0.55)" }}>
              {f.label}{f.required && <span style={{ color: "rgba(255, 210, 130, 0.9)" }}> *</span>}
            </label>
            <SlotInput field={f} value={state[f.key]} onChange={(v) => setValue(f.key, v)} />
            {f.help && <div className="text-[11px] mt-1" style={{ color: "rgba(236, 236, 241, 0.5)" }}>{f.help}</div>}
            {errors[f.key] && <div className="text-[11.5px] mt-1" style={{ color: "rgba(255, 155, 155, 0.95)" }}>{errors[f.key]}</div>}
          </div>
        ))}
      </div>
      <div className="px-4 py-3 flex justify-end" style={{ borderTop: "1px solid rgba(255, 255, 255, 0.06)" }}>
        <button
          onClick={doSubmit}
          className="px-4 py-1.5 rounded-md text-[12.5px] tracking-wide"
          style={{ background: "rgba(189, 158, 255, 0.22)", border: "1px solid rgba(189, 158, 255, 0.55)", color: "rgba(236, 236, 241, 0.94)" }}
        >
          {submit_label ?? "Continue"}
        </button>
      </div>
      {narration && (
        <div className="px-4 py-2 text-[12px]" style={{ color: "rgba(236, 236, 241, 0.65)", borderTop: "1px solid rgba(255, 255, 255, 0.06)" }}>{narration}</div>
      )}
    </div>
  );
}

function SlotInput({ field, value, onChange }: { field: SlotField; value: string | number | boolean | undefined; onChange: (v: string | number | boolean) => void }) {
  const baseStyle = {
    background: "rgba(0, 0, 0, 0.25)",
    border: "1px solid rgba(189, 158, 255, 0.32)",
    color: "rgba(236, 236, 241, 0.95)",
    colorScheme: "dark" as const,
    borderRadius: 6,
    padding: "8px 10px",
    fontSize: 13,
    outline: "none",
    width: "100%",
  };
  switch (field.type) {
    case "longtext":
      return (
        <textarea
          value={(value as string) ?? ""}
          placeholder={field.placeholder}
          onChange={(e) => onChange(e.target.value)}
          rows={3}
          style={{ ...baseStyle, resize: "vertical" }}
        />
      );
    case "number":
      return (
        <input
          type="number"
          value={(value as number | undefined) ?? ""}
          onChange={(e) => onChange(Number(e.target.value))}
          min={field.min}
          max={field.max}
          placeholder={field.placeholder}
          style={baseStyle}
        />
      );
    case "currency":
      return (
        <div style={{ position: "relative" }}>
          <span style={{ position: "absolute", left: 10, top: 8, color: "rgba(236, 236, 241, 0.55)", fontSize: 13 }}>$</span>
          <input
            type="number"
            value={(value as number | undefined) ?? ""}
            onChange={(e) => onChange(Number(e.target.value))}
            step="0.01"
            placeholder={field.placeholder}
            style={{ ...baseStyle, paddingLeft: 22 }}
          />
        </div>
      );
    case "date":
      return (
        <input
          type="date"
          value={(value as string) ?? ""}
          onChange={(e) => onChange(e.target.value)}
          style={baseStyle}
        />
      );
    case "select":
      return (
        <select
          value={(value as string) ?? ""}
          onChange={(e) => onChange(e.target.value)}
          style={baseStyle}
        >
          <option value="" disabled>{field.placeholder ?? "Choose…"}</option>
          {(field.options ?? []).map((o) => (
            <option key={o.value} value={o.value}>{o.label}</option>
          ))}
        </select>
      );
    case "checkbox":
      return (
        <label className="flex items-center gap-2 text-[13px]" style={{ color: "rgba(236, 236, 241, 0.92)" }}>
          <input
            type="checkbox"
            checked={Boolean(value)}
            onChange={(e) => onChange(e.target.checked)}
            style={{ accentColor: "rgb(189, 158, 255)" }}
          />
          {field.placeholder ?? field.label}
        </label>
      );
    case "text":
    default:
      return (
        <input
          type="text"
          value={(value as string) ?? ""}
          onChange={(e) => onChange(e.target.value)}
          placeholder={field.placeholder}
          style={baseStyle}
        />
      );
  }
}
