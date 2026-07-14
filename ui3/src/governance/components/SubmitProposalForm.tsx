import type { CSSProperties, FormEvent, KeyboardEvent, ReactNode } from "react";
import { useEffect, useMemo, useRef, useState } from "react";
import Button from "../../atoms/Button";
import { Caret, Check, ChevronLeft, Close, Search } from "../../atoms/icons";
import { MD_TOOLBAR, MdMark, charCounter } from "./SubmitFormShared";
import "./governanceform.css";

type FieldOption = { value: string; label?: ReactNode; hue?: number };
type OptionInput = string | FieldOption;

const toOption = (o: OptionInput): FieldOption =>
  typeof o === "object" && o !== null ? o : { value: o, label: o };

type AddressChip = string | { addr?: string; name?: string; hue?: number };
type CoAuthorChip = { addr?: string; hue?: number };
type CoordsValue = Record<string, string>;

type FieldValue =
  | string
  | number
  | boolean
  | string[]
  | AddressChip[]
  | CoAuthorChip[]
  | CoordsValue
  | null
  | undefined;

type OnChange = (value: FieldValue) => void;

type SubField = {
  name: string;
  label?: string;
  placeholder?: string;
  min?: number;
  max?: number;
};

type StatusLine = string | { text?: ReactNode; error?: boolean };

type FormFieldDef = {
  type?: string;
  name?: string;
  id?: string;
  label?: ReactNode;
  markdown?: boolean;
  optional?: boolean;
  disabled?: boolean;
  placeholder?: string;
  sublabel?: ReactNode;
  postlabel?: ReactNode;
  help?: ReactNode;
  className?: string;
  error?: string;
  shortError?: string;
  minLength?: number;
  maxLength?: number;
  counter?: boolean;
  counterInBar?: boolean;
  tall?: boolean;
  options?: OptionInput[];
  readOnly?: boolean;
  emptyText?: ReactNode;
  inline?: boolean;
  checkboxLabel?: ReactNode;
  search?: boolean;
  note?: ReactNode;
  max?: number;
  addLabel?: ReactNode;
  fields?: SubField[];
  rows?: number;
  unit?: ReactNode;
  unitLabel?: string;
  stepper?: boolean;
  min?: number;
  lines?: StatusLine[];
  render?: (args: { value: FieldValue; onChange: OnChange; disabled: boolean; field: FormFieldDef }) => ReactNode;
  when?: (values: Record<string, FieldValue>) => boolean;
  value?: FieldValue;
};

type FormGroup = {
  type?: never;
  section?: ReactNode;
  fields: FormFieldDef[];
  number?: number;
  isNew?: boolean;
  validated?: boolean;
};

type FormEntry = FormFieldDef | FormGroup;

const isGroup = (e: FormEntry): e is FormGroup => Boolean(e) && Array.isArray(e.fields) && !e.type;

const OkBadge = () => (
  <svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true">
    <path d="M3 8.5 6.5 12 13 4.5" stroke="currentColor" strokeWidth="2" fill="none" strokeLinecap="round" strokeLinejoin="round" />
  </svg>
);
const ErrorMark = () => (
  <svg viewBox="0 0 16 16" width="16" height="16" fill="none" aria-hidden="true">
    <circle cx="8" cy="8" r="6.5" stroke="currentColor" strokeWidth="1.4" />
    <path d="M8 4.6v4.2M8 11.2h.01" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
  </svg>
);
function FieldLabel({ field }: { field: FormFieldDef }) {
  if (!field.label) return null;
  return (
    <span className="gvf__labelrow">
      <label className="gvf__label" htmlFor={field.id}>{field.label}</label>
      {field.markdown ? (
        <sup className="gvf__notice" title="You can format your proposal using markdown! Toggle the preview switch to see how your post will be displayed.">(markdown)</sup>
      ) : null}
      {field.optional ? <sup className="gvf__optional">(optional)</sup> : null}
    </span>
  );
}

function FieldMessage({ error, current, limit }: { error?: string; current: number; limit?: number }) {
  if (!error && typeof limit !== "number") return null;
  return (
    <p className={"gvf__message" + (error ? " is-error" : "")}>
      {error ? <span className="gvf__msgerror">{error} </span> : null}
      {typeof limit === "number" ? charCounter(current, limit) : null}
    </p>
  );
}

function MarkdownField({ field, value, onChange, disabled }: { field: FormFieldDef; value: string; onChange: OnChange; disabled: boolean }) {
  const [mode, setMode] = useState<"edit" | "preview">("edit");
  const showBarCounter = field.counterInBar && typeof field.maxLength === "number";
  return (
    <div className={"gvf__md" + (disabled ? " is-disabled" : "") + (field.tall ? " gvf__md--tall" : "")}>
      <div className="gvf__mdtoolbar" role="toolbar" aria-label="Markdown commands">
        <div className="gvf__mdcmds">
          {MD_TOOLBAR.map((c) => (
            <button key={c.k} type="button" className="gvf__mdcmd" aria-label={c.k} tabIndex={-1} disabled={disabled}>
              <MdMark d={c.d} fill={c.k === "bold"} />
            </button>
          ))}
        </div>
        <div className="gvf__mdright">
          {showBarCounter ? (
            <span className="gvf__mdcount">{charCounter((value || "").length, field.maxLength ?? 0)}</span>
          ) : null}
          <button
            type="button"
            className="gvf__mdtoggle"
            onClick={() => setMode((m) => (m === "edit" ? "preview" : "edit"))}
            disabled={disabled}
            tabIndex={-1}
          >
            {mode === "edit" ? "Preview" : "Edit"}
          </button>
        </div>
      </div>
      {mode === "edit" ? (
        <textarea
          id={field.id}
          className="gvf__mdarea"
          value={value}
          placeholder={field.placeholder}
          onChange={(e) => onChange(e.target.value)}
          disabled={disabled}
        />
      ) : (
        <div className="gvf__mdpreview">{value || field.placeholder}</div>
      )}
    </div>
  );
}

function SelectField({ field, value, onChange, disabled }: { field: FormFieldDef; value: string; onChange: OnChange; disabled: boolean }) {
  const options = (field.options || []).map(toOption);
  const ro = field.readOnly;
  return (
    <div className={"gvf__selectwrap" + (ro ? " is-disabled" : "")}>
      <select
        id={field.id}
        className="gvf__select"
        value={value}
        disabled={disabled || ro}
        aria-readonly={ro || undefined}
        onChange={(e) => onChange(e.target.value)}
      >
        {field.placeholder ? (
          <option value="" disabled>{field.placeholder}</option>
        ) : null}
        {options.map((o) => (
          <option key={o.value} value={o.value}>{o.label}</option>
        ))}
      </select>
      <span className="gvf__selectcaret"><Caret size={14} strokeWidth={1.6} /></span>
    </div>
  );
}

function DropdownField({ field, value, onChange, disabled }: { field: FormFieldDef; value: string; onChange: OnChange; disabled: boolean }) {
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(-1);
  const options = (field.options || []).map(toOption);
  const selected = options.find((o) => o.value === value);
  const wrapRef = useRef<HTMLDivElement>(null);
  const btnRef = useRef<HTMLButtonElement>(null);
  const id = useRef("gvfdd" + Math.random().toString(36).slice(2, 8)).current;

  useEffect(() => {
    if (!open) return;
    function onDoc(e: MouseEvent) {
      if (wrapRef.current && e.target instanceof Node && !wrapRef.current.contains(e.target)) setOpen(false);
    }
    document.addEventListener("mousedown", onDoc);
    return () => document.removeEventListener("mousedown", onDoc);
  }, [open]);

  useEffect(() => {
    if (open) setActive(value !== undefined ? Math.max(0, options.findIndex((o) => o.value === value)) : 0);
  }, [open]);

  function pick(opt: string) {
    onChange(opt);
    setOpen(false);
    btnRef.current?.focus();
  }

  function onKey(e: KeyboardEvent<HTMLDivElement>) {
    if (e.key === "Escape") { if (open) { e.preventDefault(); setOpen(false); } return; }
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      if (!open) setOpen(true);
      else if (active >= 0) {
        const sel = options[active];
        if (sel !== undefined) pick(sel.value);
      }
      return;
    }
    if (e.key === "ArrowDown" || e.key === "ArrowUp" || e.key === "Home" || e.key === "End") {
      e.preventDefault();
      if (!open) { setOpen(true); return; }
      if (e.key === "Home") setActive(0);
      else if (e.key === "End") setActive(options.length - 1);
      else if (e.key === "ArrowDown") setActive((a) => Math.min(options.length - 1, a + 1));
      else setActive((a) => Math.max(0, a - 1));
    }
  }

  return (
    <div className="gvf__ddwrap" ref={wrapRef} onKeyDown={onKey}>
      <button
        type="button"
        className={"gvf__dropdown" + (open ? " is-open" : "")}
        disabled={disabled}
        ref={btnRef}
        onClick={() => setOpen((o) => !o)}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-activedescendant={open && active >= 0 ? id + "-" + active : undefined}
      >
        <span className={"gvf__ddtext" + (selected ? "" : " is-placeholder")}>
          {selected ? selected.label : field.placeholder || "Select"}
        </span>
        <span className="gvf__ddicon"><Caret size={14} strokeWidth={1.6} /></span>
      </button>
      {open ? (
        <ul className="gvf__menu" role="listbox">
          {options.length === 0 ? (
            <li className="gvf__menuempty">{field.emptyText || "No options"}</li>
          ) : (
            options.map((o, i) => (
              <li key={o.value} id={id + "-" + i}>
                <button
                  type="button"
                  role="option"
                  aria-selected={o.value === value}
                  className={"gvf__option" + (o.value === value ? " is-selected" : "") + (i === active ? " is-active" : "")}
                  onClick={() => pick(o.value)}
                >
                  {typeof o.hue === "number" ? (
                    <span className="u-avatar" style={{ "--sz": "22px", "--hue": o.hue } as CSSProperties & { "--sz": string; "--hue": number }} aria-hidden="true" />
                  ) : null}
                  {o.label}
                </button>
              </li>
            ))
          )}
        </ul>
      ) : null}
    </div>
  );
}

function RadioGroup({ field, value, onChange, disabled }: { field: FormFieldDef; value: FieldValue; onChange: OnChange; disabled: boolean }) {
  const options = (field.options || []).map(toOption);
  return (
    <div className={"gvf__radios" + (field.inline ? " gvf__radios--inline" : "")}>
      {options.map((o) => (
        <label key={o.value} className="gvf__radio">
          <input
            type="radio"
            name={field.name}
            checked={value === o.value}
            disabled={disabled}
            onChange={() => onChange(o.value)}
          />
          <span className="gvf__radiomark" aria-hidden="true" />
          {o.label}
        </label>
      ))}
    </div>
  );
}

function PillsField({ field, value, onChange, disabled }: { field: FormFieldDef; value: FieldValue; onChange: OnChange; disabled: boolean }) {
  const options = (field.options || []).map(toOption);
  return (
    <div className="gvf__pills">
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          className={"gvf__pill" + (value === o.value ? " is-selected gvf__pill--" + o.value : "")}
          disabled={disabled}
          onClick={() => onChange(o.value)}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

function CheckboxField({ field, value, onChange, disabled }: { field: FormFieldDef; value: FieldValue; onChange: OnChange; disabled: boolean }) {
  const options = field.options ? field.options.map(toOption) : null;
  if (options) {
    const set = new Set<string>(Array.isArray(value) ? (value as string[]) : []);
    const toggle = (v: string) => {
      const next = new Set(set);
      next.has(v) ? next.delete(v) : next.add(v);
      onChange([...next]);
    };
    return (
      <div className="gvf__checks">
        {options.map((o) => (
          <button
            key={o.value}
            type="button"
            role="checkbox"
            aria-checked={set.has(o.value)}
            className={"gvf__check" + (set.has(o.value) ? " is-checked" : "")}
            disabled={disabled}
            onClick={() => toggle(o.value)}
          >
            <span className="gvf__checkbox">{set.has(o.value) ? <Check size={12} strokeWidth={3} /> : null}</span>
            <span className="gvf__checktext">{o.label}</span>
          </button>
        ))}
      </div>
    );
  }
  return (
    <button
      type="button"
      role="checkbox"
      aria-checked={!!value}
      className={"gvf__check" + (value ? " is-checked" : "")}
      disabled={disabled}
      onClick={() => onChange(!value)}
    >
      <span className="gvf__checkbox">{value ? <Check size={12} strokeWidth={3} /> : null}</span>
      <span className="gvf__checktext">{field.checkboxLabel || field.label}</span>
    </button>
  );
}

const chipLabel = (c: AddressChip): ReactNode => (typeof c === "string" ? c : c.addr || c.name || "");

function AddressField({ field, value, onChange, disabled }: { field: FormFieldDef; value: FieldValue; onChange: OnChange; disabled: boolean }) {
  const chips: AddressChip[] = Array.isArray(value) ? (value as AddressChip[]) : [];
  return (
    <div className="gvf__addrfield">
      <div className={"gvf__addrselect" + (disabled ? " is-disabled" : "")}>
        {chips.map((c, i) => (
          <span className="gvf__coauthor" key={i}>
            {typeof c === "object" && typeof c.hue === "number" ? (
              <span className="u-avatar" style={{ "--sz": "20px", "--hue": c.hue } as CSSProperties & { "--sz": string; "--hue": number }} aria-hidden="true" />
            ) : null}
            {chipLabel(c)}
            <button
              type="button"
              className="gvf__coremove"
              aria-label="Remove address"
              disabled={disabled}
              onClick={() => onChange(chips.filter((_, idx) => idx !== i))}
            >
              <Close size={12} strokeWidth={2.55} />
            </button>
          </span>
        ))}
        {field.search ? <span className="gvf__addricon"><Search size={15} strokeWidth={2.25} /></span> : null}
        <input
          className="gvf__addrinput"
          type="text"
          placeholder={chips.length ? "" : field.placeholder}
          aria-label={typeof field.label === "string" ? field.label : "Address"}
          disabled={disabled}
        />
      </div>
      {field.note ? <p className="gvf__addrmsg">{field.note}</p> : null}
    </div>
  );
}

function ListField({ field, value, onChange, disabled }: { field: FormFieldDef; value: FieldValue; onChange: OnChange; disabled: boolean }) {
  const rows: string[] = Array.isArray(value) && value.length ? (value as string[]) : [""];
  const max = field.max;
  const setRow = (i: number, v: string) => onChange(rows.map((r, idx) => (idx === i ? v : r)));
  const addRow = () => {
    if (max != null && rows.length >= max) return;
    onChange([...rows, ""]);
  };
  const removeRow = (i: number) => {
    const next = rows.filter((_, idx) => idx !== i);
    onChange(next.length ? next : [""]);
  };
  const canAdd = max == null || rows.length < max;
  return (
    <div className="gvf__list">
      {rows.map((r, i) => (
        <div key={i} className="gvf__listrow">
          <input
            className="gvf__input"
            type="text"
            placeholder={field.placeholder}
            value={r}
            disabled={disabled}
            onChange={(e) => setRow(i, e.target.value)}
          />
          <button
            type="button"
            className="gvf__listremove"
            aria-label="Remove"
            disabled={disabled}
            onClick={() => removeRow(i)}
          >
            <Close size={13} strokeWidth={2.55} />
          </button>
        </div>
      ))}
      {canAdd ? (
        <button type="button" className="gvf__listadd" disabled={disabled} onClick={addRow}>
          {field.addLabel || "Add another"}
        </button>
      ) : null}
    </div>
  );
}

function CoordsField({ field, value, onChange, disabled }: { field: FormFieldDef; value: FieldValue; onChange: OnChange; disabled: boolean }) {
  const subs: SubField[] = field.fields || [
    { name: "x", placeholder: "-150 through 150" },
    { name: "y", placeholder: "-150 through 150" },
  ];
  const v: CoordsValue = value && typeof value === "object" && !Array.isArray(value) ? (value as CoordsValue) : {};
  return (
    <div className="gvf__coords">
      {subs.map((s) => (
        <input
          key={s.name}
          className="gvf__coordinput"
          type="number"
          inputMode="numeric"
          min={s.min}
          max={s.max}
          placeholder={s.placeholder}
          aria-label={s.label || s.name}
          value={v[s.name] ?? ""}
          disabled={disabled}
          onChange={(e) => onChange({ ...v, [s.name]: e.target.value })}
        />
      ))}
      {field.error ? <div className="gvf__coorderror">{field.error}</div> : null}
    </div>
  );
}

function CoAuthorsField({ field, value, onChange, disabled }: { field: FormFieldDef; value: FieldValue; onChange: OnChange; disabled: boolean }) {
  const chips: CoAuthorChip[] = Array.isArray(value) ? (value as CoAuthorChip[]) : [];
  const max = field.max ?? 5;
  const addCoAuthor = () => {
    if (chips.length >= max) return;
    onChange([
      ...chips,
      {
        addr: "0x" + Math.random().toString(16).slice(2, 6) + "\u{2026}" + Math.random().toString(16).slice(2, 6),
        hue: Math.floor(Math.random() * 360),
      },
    ]);
  };
  const removeCoAuthor = (i: number) => onChange(chips.filter((_, idx) => idx !== i));
  return (
    <div className="gvf__coauthors">
      {chips.map((c, i) => (
        <span className="gvf__coauthor" key={i}>
          <span className="u-avatar" style={{ "--sz": "22px", "--hue": c.hue } as CSSProperties & { "--sz": string; "--hue": number | undefined }} aria-hidden="true" />
          {c.addr}
          <button type="button" className="gvf__coremove" aria-label="Remove co-author" disabled={disabled} onClick={() => removeCoAuthor(i)}>
            <Close size={12} strokeWidth={2.55} />
          </button>
        </span>
      ))}
      {chips.length < max ? (
        <button type="button" className="gvf__coadd" disabled={disabled} onClick={addCoAuthor}>
          {chips.length === 0 ? (field.placeholder || "Add co-authors") : "+ Add co-author"}
        </button>
      ) : null}
    </div>
  );
}

function FormField({ field, value, onChange, disabled }: { field: FormFieldDef; value: FieldValue; onChange: OnChange; disabled: boolean }) {
  const fieldDisabled = disabled || Boolean(field.disabled);
  const control = (() => {
    switch (field.type) {
      case "text":
      case "email":
        return (
          <input
            id={field.id}
            className="gvf__input"
            type={field.type === "email" ? "email" : "text"}
            placeholder={field.placeholder}
            value={(value ?? "") as string}
            maxLength={field.maxLength}
            disabled={fieldDisabled}
            onChange={(e) => onChange(e.target.value)}
          />
        );
      case "number":
        if (field.stepper) {
          const num = Number(value || 0);
          const min = field.min ?? 0;
          const max = field.max ?? Infinity;
          return (
            <div className="gvf__stepper">
              <button
                type="button"
                className="gvf__stepbtn"
                aria-label="Decrease"
                disabled={fieldDisabled || num <= min}
                onClick={() => onChange(Math.max(min, num - 1))}
              >
                &#x2212;
              </button>
              <span className="gvf__stepval">{num}{field.unitLabel ? " " + field.unitLabel : ""}</span>
              <button
                type="button"
                className="gvf__stepbtn"
                aria-label="Increase"
                disabled={fieldDisabled || num >= max}
                onClick={() => onChange(Math.min(max, num + 1))}
              >
                +
              </button>
            </div>
          );
        }
        return (
          <div className="gvf__numwrap">
            {field.unit ? <span className="gvf__numunit">{field.unit}</span> : null}
            <input
              id={field.id}
              className={"gvf__input" + (field.unit ? " gvf__input--unit" : "")}
              type="number"
              inputMode="numeric"
              placeholder={field.placeholder}
              value={(value ?? "") as string}
              min={field.min}
              max={field.max}
              disabled={fieldDisabled}
              onChange={(e) => onChange(e.target.value)}
            />
          </div>
        );
      case "textarea":
        return (
          <textarea
            id={field.id}
            className="gvf__textarea"
            placeholder={field.placeholder}
            value={(value ?? "") as string}
            maxLength={field.maxLength}
            rows={field.rows || 5}
            disabled={fieldDisabled}
            onChange={(e) => onChange(e.target.value)}
          />
        );
      case "markdown":
        return <MarkdownField field={field} value={(value ?? "") as string} onChange={onChange} disabled={fieldDisabled} />;
      case "select":
      case "date":
        return <SelectField field={field} value={(value ?? "") as string} onChange={onChange} disabled={fieldDisabled} />;
      case "dropdown":
        return <DropdownField field={field} value={(value ?? "") as string} onChange={onChange} disabled={fieldDisabled} />;
      case "radio":
      case "token":
        return <RadioGroup field={field} value={value} onChange={onChange} disabled={fieldDisabled} />;
      case "pills":
        return <PillsField field={field} value={value} onChange={onChange} disabled={fieldDisabled} />;
      case "checkbox":
        return <CheckboxField field={field} value={value} onChange={onChange} disabled={fieldDisabled} />;
      case "address":
        return <AddressField field={field} value={value} onChange={onChange} disabled={fieldDisabled} />;
      case "list":
        return <ListField field={field} value={value} onChange={onChange} disabled={fieldDisabled} />;
      case "coords":
        return <CoordsField field={field} value={value} onChange={onChange} disabled={fieldDisabled} />;
      case "coauthors":
        return <CoAuthorsField field={field} value={value} onChange={onChange} disabled={fieldDisabled} />;
      case "status":
        return (
          <div className="gvf__status">
            {(field.lines || []).map((l, i) => (
              <span key={i} className={"gvf__statusline" + (typeof l !== "string" && l.error ? " is-error" : " is-ok")}>
                <span className="gvf__statusmark"><OkBadge /></span>
                {typeof l === "string" ? l : l.text}
              </span>
            ))}
          </div>
        );
      case "custom":
        return field.render ? field.render({ value, onChange, disabled: fieldDisabled, field }) : null;
      default:
        return null;
    }
  })();

  const stringLen = typeof value === "string" ? value.length : 0;
  const wantsCounter =
    field.counter !== false &&
    !field.counterInBar &&
    typeof field.maxLength === "number" &&
    (field.type === "text" || field.type === "email" || field.type === "textarea" || field.type === "markdown");

  const tooShort =
    typeof field.minLength === "number" &&
    typeof value === "string" &&
    value.length > 0 &&
    value.length < field.minLength;
  const error = field.error || (tooShort ? field.shortError || "This field is too short." : undefined);

  return (
    <section className={"gvf__section" + (fieldDisabled ? " is-disabled" : "") + (field.className ? " " + field.className : "")}>
      <FieldLabel field={field} />
      {field.sublabel ? <p className="gvf__sublabel">{field.sublabel}</p> : null}
      {control}
      {field.postlabel ? <p className="gvf__postlabel">{field.postlabel}</p> : null}
      {field.help ? <p className="gvf__help">{field.help}</p> : null}
      <FieldMessage
        error={error}
        current={stringLen}
        limit={wantsCounter ? field.maxLength : undefined}
      />
    </section>
  );
}

function SectionHeader({ number, title, isNew, validated }: { number: number; title?: ReactNode; isNew?: boolean; validated?: boolean }) {
  return (
    <div className="gvf__grouphead">
      <span className="gvf__groupicon" aria-hidden="true">
        {validated ? <span className="gvf__grouptick"><OkBadge /></span> : null}
        <span className="gvf__groupnum">{number}</span>
      </span>
      <span className="gvf__grouptitle">
        {title}
        {isNew ? <span className="gvf__newbadge">New</span> : null}
      </span>
      <span className="gvf__grouprule" aria-hidden="true" />
    </div>
  );
}

function defaultValue(field: FormFieldDef): FieldValue {
  if ("value" in field) return field.value;
  switch (field.type) {
    case "coauthors":
    case "address":
      return [];
    case "list":
      return [""];
    case "coords":
      return {};
    case "checkbox":
      return field.options ? [] : false;
    case "number":
      return field.stepper ? field.min ?? 0 : "";
    case "radio":
    case "token":
    case "pills": {
      const first = field.options?.[0];
      return first !== undefined ? toOption(first).value : "";
    }
    default:
      return "";
  }
}

type PreparedFieldEntry = { field: FormFieldDef };
type PreparedGroupEntry = {
  field?: undefined;
  section?: ReactNode;
  fields: FormFieldDef[];
  number: number;
  isNew?: boolean;
  validated?: boolean;
};
type PreparedEntry = PreparedFieldEntry | PreparedGroupEntry;

type SubmitProposalFormProps = {
  title?: ReactNode;
  subtitle?: ReactNode;
  description?: ReactNode | ReactNode[];
  sections?: FormEntry[];
  numbered?: boolean;
  backHref?: string;
  onBack?: () => void;
  showBack?: boolean;
  submitLabel?: ReactNode;
  secondaryLabel?: ReactNode;
  onSecondary?: () => void;
  submitDisabled?: boolean;
  onSubmit?: (values: Record<string, FieldValue>) => void;
  disabled?: boolean;
  vpNotice?: ReactNode;
  error?: ReactNode;
  errorLabel?: ReactNode;
  errorCollapsible?: boolean;
  values?: Record<string, FieldValue>;
  onChange?: (name: string, value: FieldValue, values: Record<string, FieldValue>) => void;
  className?: string;
};

export default function SubmitProposalForm({
  title,
  subtitle,
  description,
  sections = [],
  numbered = false,
  backHref,
  onBack,
  showBack,
  submitLabel = "Submit proposal",
  secondaryLabel,
  onSecondary,
  submitDisabled = false,
  onSubmit,
  disabled = false,
  vpNotice,
  error,
  errorLabel = "There was an error.",
  errorCollapsible = false,
  values: controlledValues,
  onChange,
  className,
}: SubmitProposalFormProps) {
  const [errorOpen, setErrorOpen] = useState(false);

  const { groups, allFields } = useMemo(() => {
    let auto = 0;
    let idx = 0;
    const prep = (f: FormFieldDef): FormFieldDef => {
      const name = f.name ?? "field_" + idx;
      const out = { ...f, name, id: f.id ?? "gvf-" + name };
      idx += 1;
      return out;
    };
    const gs: PreparedEntry[] = [];
    const flat: FormFieldDef[] = [];
    sections.forEach((entry) => {
      if (isGroup(entry)) {
        const fields = entry.fields.map(prep);
        flat.push(...fields);
        auto += 1;
        gs.push({ ...entry, number: entry.number ?? auto, fields });
      } else {
        const field = prep(entry);
        flat.push(field);
        gs.push({ field });
      }
    });
    return { groups: gs, allFields: flat };
  }, [sections]);

  const [localValues, setLocalValues] = useState<Record<string, FieldValue>>(() => {
    const seed: Record<string, FieldValue> = {};
    allFields.forEach((f) => {
      seed[f.name ?? ""] = defaultValue(f);
    });
    return seed;
  });

  const values = controlledValues ?? localValues;

  const setValue = (name: string, val: FieldValue) => {
    if (!controlledValues) {
      setLocalValues((v) => ({ ...v, [name]: val }));
    }
    onChange?.(name, val, { ...values, [name]: val });
  };

  const hasBack = showBack ?? (backHref != null || onBack != null);
  const hasAside = title != null || subtitle != null || description != null;

  const handleSubmit = (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    if (disabled || submitDisabled) return;
    onSubmit?.(values);
  };

  const renderField = (f: FormFieldDef) =>
    f.when && !f.when(values) ? null : (
      <FormField
        key={f.name}
        field={f}
        value={values[f.name ?? ""]}
        onChange={(val) => setValue(f.name ?? "", val)}
        disabled={disabled}
      />
    );

  return (
    <div className={"gvf" + (hasBack ? " gvf--withback" : "") + (className ? " " + className : "")}>
      {hasBack ? (
        <div className="gvf__back">
          <a
            className="gvf__backbtn"
            href={backHref || undefined}
            role={backHref ? undefined : "button"}
            aria-label="Back"
            onClick={(e) => {
              if (onBack) {
                e.preventDefault();
                onBack();
              }
            }}
          >
            <ChevronLeft size={16} strokeWidth={2.7} />
          </a>
        </div>
      ) : null}

      <form className={"gvf__form" + (hasAside ? "" : " gvf__form--single")} onSubmit={handleSubmit}>
        {hasAside ? (
          <aside className="gvf__aside">
            {title ? (
              <section className="gvf__section gvf__section--title">
                <h1 className="gvf__h1">{title}</h1>
                {subtitle ? <p className="gvf__lead">{subtitle}</p> : null}
              </section>
            ) : null}

            {description != null ? (
              <section className="gvf__section gvf__section--intro">
                {Array.isArray(description)
                  ? description.map((p, i) => <p key={i} className="gvf__intro">{p}</p>)
                  : typeof description === "string"
                    ? <p className="gvf__intro">{description}</p>
                    : <div className="gvf__intro">{description}</div>}
              </section>
            ) : null}
          </aside>
        ) : null}

        <div className="gvf__main">
        {groups.map((g, gi) =>
          g.field ? (
            renderField(g.field)
          ) : (
            <section key={"grp-" + gi} className="gvf__group">
              <SectionHeader
                number={numbered || g.number != null ? g.number : gi + 1}
                title={g.section}
                isNew={g.isNew}
                validated={g.validated}
              />
              <div className="gvf__groupbody">{g.fields.map(renderField)}</div>
            </section>
          )
        )}

        <section className="gvf__section gvf__section--submit">
          <Button type="submit" variant="primary" className="gvf__submit" disabled={disabled || submitDisabled}>
            {submitLabel}
          </Button>
          {secondaryLabel ? (
            <Button type="button" variant="secondary" className="gvf__secondary" disabled={disabled} onClick={onSecondary}>
              {secondaryLabel}
            </Button>
          ) : null}
        </section>

        {vpNotice ? (
          <section className="gvf__section">
            <p className="gvf__vpnotice">{vpNotice}</p>
          </section>
        ) : null}

        {error ? (
          <section className="gvf__section">
            <div className="gvf__error" role="alert">
              <span className="gvf__erroricon"><ErrorMark /></span>
              <div className="gvf__errorbody">
                <div className="gvf__errorhead">
                  <p className="gvf__errorlabel">{errorLabel}</p>
                  {errorCollapsible ? (
                    <button type="button" className="gvf__errortoggle" onClick={() => setErrorOpen((o) => !o)}>
                      {errorOpen ? "Hide" : "Show"}
                    </button>
                  ) : null}
                </div>
                {!errorCollapsible || errorOpen ? <p className="gvf__errormsg">{error}</p> : null}
              </div>
            </div>
          </section>
        ) : null}
        </div>
      </form>
    </div>
  );
}
