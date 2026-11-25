import { AlarmClockCheck, AlarmClockOff, Info } from "lucide-react";
import { useEffect, useState } from "react";
import { useOutletContext } from "react-router";
import Button from "~/components/next/Button";
import { acct, api } from "~/lib/api";
import { t } from "~/lib/i18n";
import type { ShellCtx } from "../shell";

// mirrors chatwoot/app/javascript/dashboard/routes/dashboard/settings/sla/
type Unit = "Minutes" | "Hours" | "Days";
type SlaPolicy = {
  id: number;
  name: string;
  description: string | null;
  first_response_time_threshold: number | null;
  next_response_time_threshold: number | null;
  resolution_time_threshold: number | null;
  only_during_business_hours: boolean;
};
type Form = {
  name: string;
  description: string;
  frt: string;
  frtUnit: Unit;
  nrt: string;
  nrtUnit: Unit;
  rt: string;
  rtUnit: Unit;
  only_during_business_hours: boolean;
};

const empty: Form = {
  name: "",
  description: "",
  frt: "",
  frtUnit: "Minutes",
  nrt: "",
  nrtUnit: "Minutes",
  rt: "",
  rtUnit: "Minutes",
  only_during_business_hours: false,
};

const unitSeconds: Record<Unit, number> = { Minutes: 60, Hours: 3600, Days: 86400 };
const unitOptions: { value: Unit; label: string }[] = [
  { value: "Minutes", label: "minutes" },
  { value: "Hours", label: "hours" },
  { value: "Days", label: "days" },
];

function fromSeconds(seconds: number | null | undefined): { value: string; unit: Unit } {
  if (seconds == null || seconds <= 0) return { value: "", unit: "Minutes" };
  if (seconds < 3600) return { value: String(Number((seconds / 60).toFixed(1))), unit: "Minutes" };
  if (seconds < 86400) return { value: String(Number((seconds / 3600).toFixed(1))), unit: "Hours" };
  return { value: String(Number((seconds / 86400).toFixed(1))), unit: "Days" };
}

function toSeconds(value: string, unit: Unit): number | null {
  if (!value.trim()) return null;
  const num = Number(value);
  if (!Number.isFinite(num) || num <= 0) return null;
  return Number(num * unitSeconds[unit]);
}

function displayTime(seconds: number | null | undefined): string {
  if (seconds == null || seconds <= 0) return "-";
  if (seconds < 3600) return `${Number((seconds / 60).toFixed(1))}m`;
  if (seconds < 86400) return `${Number((seconds / 3600).toFixed(1))}h`;
  return `${Number((seconds / 86400).toFixed(1))}d`;
}

function validThreshold(value: string): boolean {
  if (!value.trim()) return true;
  const num = Number(value);
  return Number.isFinite(num) && num >= 0.001;
}

export default function SlaSettings() {
  const { accountId } = useOutletContext<ShellCtx>();
  const base = `${acct(accountId)}/sla_policies`;
  const [items, setItems] = useState<SlaPolicy[]>([]);
  const [loading, setLoading] = useState(true);
  const [search, setSearch] = useState("");
  const [selected, setSelected] = useState<SlaPolicy | null>(null);
  const [mode, setMode] = useState<"add" | "edit" | "delete" | null>(null);
  const [form, setForm] = useState<Form>(empty);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  async function load() {
    const data = await api<SlaPolicy[] | { payload: SlaPolicy[] }>(base);
    setItems(Array.isArray(data) ? data : (data.payload ?? []));
  }

  useEffect(() => {
    setLoading(true);
    load().catch(e => setError(e.message)).finally(() => setLoading(false));
  }, [base]);

  function open(item?: SlaPolicy) {
    setError("");
    setSelected(item ?? null);
    if (item) {
      const frt = fromSeconds(item.first_response_time_threshold);
      const nrt = fromSeconds(item.next_response_time_threshold);
      const rt = fromSeconds(item.resolution_time_threshold);
      setForm({
        name: item.name,
        description: item.description ?? "",
        frt: frt.value,
        frtUnit: frt.unit,
        nrt: nrt.value,
        nrtUnit: nrt.unit,
        rt: rt.value,
        rtUnit: rt.unit,
        only_during_business_hours: item.only_during_business_hours,
      });
      setMode("edit");
    } else {
      setForm(empty);
      setMode("add");
    }
  }

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError("");
    const payload = {
      name: form.name.trim(),
      description: form.description.trim() || null,
      first_response_time_threshold: toSeconds(form.frt, form.frtUnit),
      next_response_time_threshold: toSeconds(form.nrt, form.nrtUnit),
      resolution_time_threshold: toSeconds(form.rt, form.rtUnit),
      only_during_business_hours: form.only_during_business_hours,
    };
    try {
      await api(mode === "add" ? base : `${base}/${selected!.id}`, {
        method: mode === "add" ? "POST" : "PATCH",
        json: payload,
      });
      await load();
      setMode(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : t("SLA.ADD.API.ERROR_MESSAGE"));
    } finally {
      setBusy(false);
    }
  }

  async function remove() {
    if (!selected) return;
    setBusy(true);
    setError("");
    try {
      await api(`${base}/${selected.id}`, { method: "DELETE" });
      await load();
      setMode(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : t("SLA.DELETE.API.ERROR_MESSAGE"));
    } finally {
      setBusy(false);
    }
  }

  const filtered = items.filter(item =>
    `${item.name} ${item.description ?? ""}`.toLowerCase().includes(search.trim().toLowerCase()),
  );
  const valid =
    form.name.trim().length >= 2 &&
    validThreshold(form.frt) &&
    validThreshold(form.nrt) &&
    validThreshold(form.rt);

  const countLabel = items.length > 0
    ? t("SLA.COUNT", { n: items.length }).split(" | ")[items.length === 1 ? 0 : 1]
    : "";

  const thresholdFields: {
    label: string;
    placeholder: string;
    valueKey: "frt" | "nrt" | "rt";
    unitKey: "frtUnit" | "nrtUnit" | "rtUnit";
  }[] = [
    {
      label: t("SLA.FORM.FIRST_RESPONSE_TIME.LABEL"),
      placeholder: t("SLA.FORM.FIRST_RESPONSE_TIME.PLACEHOLDER"),
      valueKey: "frt",
      unitKey: "frtUnit",
    },
    {
      label: t("SLA.FORM.NEXT_RESPONSE_TIME.LABEL"),
      placeholder: t("SLA.FORM.NEXT_RESPONSE_TIME.PLACEHOLDER"),
      valueKey: "nrt",
      unitKey: "nrtUnit",
    },
    {
      label: t("SLA.FORM.RESOLUTION_TIME.LABEL"),
      placeholder: t("SLA.FORM.RESOLUTION_TIME.PLACEHOLDER"),
      valueKey: "rt",
      unitKey: "rtUnit",
    },
  ];

  return (
    <section className="flex-1 overflow-auto p-8 text-n-slate-12">
      <header className="mb-6 flex flex-wrap items-start justify-between gap-4">
        <div>
          <h1 className="text-xl font-medium">{t("SLA.HEADER")}</h1>
          <p className="mt-2 max-w-2xl text-sm text-n-slate-11">{t("SLA.DESCRIPTION")}</p>
          <a
            className="text-sm text-n-blue-11"
            href="https://chwt.app/hc/sla"
            target="_blank"
            rel="noreferrer"
          >
            {t("SLA.LEARN_MORE")}
          </a>
        </div>
        <Button sm label={t("SLA.ADD_ACTION")} onClick={() => open()} />
      </header>

      <div className="mb-4 flex items-center justify-between gap-3">
        <span className="text-sm text-n-slate-11">{countLabel}</span>
        <input
          aria-label={t("SLA.SEARCH_PLACEHOLDER")}
          placeholder={t("SLA.SEARCH_PLACEHOLDER")}
          value={search}
          onChange={e => setSearch(e.target.value)}
          className="rounded-lg border border-n-weak bg-n-background px-3 py-2 text-sm"
        />
      </div>

      {error && !mode && (
        <p role="alert" className="mb-4 text-n-ruby-11">
          {error}
        </p>
      )}

      {loading ? (
        <p>{t("SLA.LOADING")}</p>
      ) : items.length === 0 ? (
        <p className="text-sm text-n-slate-11">{t("SLA.LIST.404")}</p>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full text-left text-sm">
            <thead>
              <tr className="border-b border-n-weak text-n-slate-11">
                <th className="p-3 font-medium">{t("SLA.LIST.TABLE_HEADER.SLA")}</th>
                <th className="p-3 font-medium">{t("SLA.LIST.TABLE_HEADER.BUSINESS_HOURS")}</th>
                <th className="p-3 font-medium">
                  <span className="inline-flex items-center gap-1" title={t("SLA.LIST.RESPONSE_TYPES.FRT")}>
                    {t("SLA.LIST.RESPONSE_TYPES.SHORT_HAND.FRT")}
                    <Info className="size-3.5 text-n-slate-10" aria-label={t("SLA.LIST.RESPONSE_TYPES.FRT")} />
                  </span>
                </th>
                <th className="p-3 font-medium">
                  <span className="inline-flex items-center gap-1" title={t("SLA.LIST.RESPONSE_TYPES.NRT")}>
                    {t("SLA.LIST.RESPONSE_TYPES.SHORT_HAND.NRT")}
                    <Info className="size-3.5 text-n-slate-10" aria-label={t("SLA.LIST.RESPONSE_TYPES.NRT")} />
                  </span>
                </th>
                <th className="p-3 font-medium">
                  <span className="inline-flex items-center gap-1" title={t("SLA.LIST.RESPONSE_TYPES.RT")}>
                    {t("SLA.LIST.RESPONSE_TYPES.SHORT_HAND.RT")}
                    <Info className="size-3.5 text-n-slate-10" aria-label={t("SLA.LIST.RESPONSE_TYPES.RT")} />
                  </span>
                </th>
                <th className="p-3 text-right font-medium">{t("INTEGRATION_APPS.LIST.ACTIONS")}</th>
              </tr>
            </thead>
            <tbody>
              {filtered.map(item => (
                <tr key={item.id} className="border-b border-n-weak">
                  <td className="p-3">
                    <div className="flex min-w-0 flex-col gap-1">
                      <span className="truncate font-medium text-n-slate-12">{item.name}</span>
                      {item.description && (
                        <span className="line-clamp-1 text-n-slate-11">{item.description}</span>
                      )}
                    </div>
                  </td>
                  <td className="whitespace-nowrap p-3">
                    <span className="inline-flex items-center gap-1.5 rounded-md border border-n-weak px-2 py-0.5 text-xs">
                      {item.only_during_business_hours ? (
                        <>
                          <AlarmClockCheck className="size-3.5 text-n-teal-11" />
                          <span>{t("SLA.LIST.BUSINESS_HOURS_ON")}</span>
                        </>
                      ) : (
                        <>
                          <AlarmClockOff className="size-3.5 text-n-slate-11" />
                          <span className="text-n-slate-11">{t("SLA.LIST.BUSINESS_HOURS_OFF")}</span>
                        </>
                      )}
                    </span>
                  </td>
                  <td className="whitespace-nowrap p-3 tabular-nums">
                    {displayTime(item.first_response_time_threshold)}
                  </td>
                  <td className="whitespace-nowrap p-3 tabular-nums">
                    {displayTime(item.next_response_time_threshold)}
                  </td>
                  <td className="whitespace-nowrap p-3 tabular-nums">
                    {displayTime(item.resolution_time_threshold)}
                  </td>
                  <td className="p-3">
                    <div className="flex justify-end gap-3">
                      <Button
                        sm
                        slate
                        ghost
                        aria-label={t("SLA.FORM.EDIT")}
                        icon="pencil"
                        onClick={() => open(item)}
                      />
                      <Button
                        sm
                        slate
                        ghost
                        aria-label={t("SLA.FORM.DELETE")}
                        icon="trash-2"
                        onClick={() => {
                          setError("");
                          setSelected(item);
                          setMode("delete");
                        }}
                      />
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          {filtered.length === 0 && <p className="p-4">{t("SLA.SEARCH.NO_RESULTS")}</p>}
        </div>
      )}

      {mode && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"
          onMouseDown={e => {
            if (e.target === e.currentTarget && !busy) setMode(null);
          }}
        >
          <div
            role="dialog"
            aria-modal="true"
            aria-labelledby="sla-dialog-title"
            className="w-full max-w-lg rounded-xl bg-n-background p-6 shadow-xl"
          >
            <h2 id="sla-dialog-title" className="text-lg font-medium">
              {mode === "add"
                ? t("SLA.ADD.TITLE")
                : mode === "edit"
                  ? `${t("SLA.FORM.EDIT")} - ${selected?.name}`
                  : t("SLA.DELETE.CONFIRM.TITLE")}
            </h2>
            {mode === "delete" ? (
              <>
                <p className="my-4">
                  {t("SLA.DELETE.CONFIRM.MESSAGE")} {selected?.name}?
                </p>
                {error && (
                  <p role="alert" className="text-n-ruby-11">
                    {error}
                  </p>
                )}
                <div className="mt-6 flex justify-end gap-2">
                  <Button slate faded label={t("SLA.DELETE.CONFIRM.NO")} onClick={() => setMode(null)} />
                  <Button
                    ruby
                    label={t("SLA.DELETE.CONFIRM.YES")}
                    disabled={busy}
                    onClick={remove}
                  />
                </div>
              </>
            ) : (
              <form onSubmit={submit} className="mt-4 grid gap-4">
                {mode === "add" && <p className="text-sm text-n-slate-11">{t("SLA.ADD.DESC")}</p>}
                <label className="grid gap-1 text-sm">
                  {t("SLA.FORM.NAME.LABEL")}
                  <input
                    autoFocus
                    required
                    minLength={2}
                    value={form.name}
                    onChange={e => setForm({ ...form, name: e.target.value })}
                    placeholder={t("SLA.FORM.NAME.PLACEHOLDER")}
                    className="rounded-lg border border-n-weak bg-n-background px-3 py-2"
                  />
                </label>
                <label className="grid gap-1 text-sm">
                  {t("SLA.FORM.DESCRIPTION.LABEL")}
                  <input
                    value={form.description}
                    onChange={e => setForm({ ...form, description: e.target.value })}
                    placeholder={t("SLA.FORM.DESCRIPTION.PLACEHOLDER")}
                    className="rounded-lg border border-n-weak bg-n-background px-3 py-2"
                  />
                </label>
                {thresholdFields.map(({ label, placeholder, valueKey, unitKey }) => {
                  const val = form[valueKey];
                  const invalid = !validThreshold(val);
                  return (
                    <div key={valueKey} className="grid gap-1 text-sm">
                      <label htmlFor={`sla-${valueKey}`}>{label}</label>
                      <div className="flex items-center gap-3">
                        <input
                          id={`sla-${valueKey}`}
                          type="number"
                          step="any"
                          min="0.001"
                          value={val}
                          onChange={e => setForm({ ...form, [valueKey]: e.target.value })}
                          placeholder={placeholder}
                          className="min-w-0 flex-1 rounded-lg border border-n-weak bg-n-background px-3 py-2"
                        />
                        <select
                          aria-label={`${label} unit`}
                          value={form[unitKey]}
                          onChange={e => setForm({ ...form, [unitKey]: e.target.value as Unit })}
                          className="rounded-lg border border-n-weak bg-n-background px-3 py-2"
                        >
                          {unitOptions.map(opt => (
                            <option key={opt.value} value={opt.value}>
                              {opt.label}
                            </option>
                          ))}
                        </select>
                      </div>
                      {invalid && (
                        <span className="text-xs text-n-ruby-11">
                          {t("SLA.FORM.THRESHOLD_TIME.INVALID_FORMAT_ERROR")}
                        </span>
                      )}
                    </div>
                  );
                })}
                <label className="flex items-center justify-between gap-2 rounded-lg border border-n-weak px-3 py-2 text-sm">
                  <span className="text-n-slate-11">{t("SLA.FORM.BUSINESS_HOURS.PLACEHOLDER")}</span>
                  <input
                    type="checkbox"
                    checked={form.only_during_business_hours}
                    onChange={e =>
                      setForm({ ...form, only_during_business_hours: e.target.checked })
                    }
                  />
                </label>
                {error && (
                  <p role="alert" className="text-n-ruby-11">
                    {error}
                  </p>
                )}
                <div className="flex justify-end gap-2">
                  <Button
                    type="button"
                    slate
                    faded
                    label={t("SLA.FORM.CANCEL")}
                    onClick={() => setMode(null)}
                  />
                  <Button
                    type="submit"
                    disabled={!valid || busy}
                    label={t(mode === "add" ? "SLA.FORM.CREATE" : "SLA.FORM.EDIT")}
                  />
                </div>
              </form>
            )}
          </div>
        </div>
      )}
    </section>
  );
}
