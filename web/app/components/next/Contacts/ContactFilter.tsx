import { useState, useRef, useEffect } from "react";
import Button from "~/components/next/Button";
import { t } from "~/lib/i18n";

export type FilterCondition = {
  attribute_key: "name" | "email" | "phone_number" | "identifier";
  filter_operator:
    | "equal_to"
    | "not_equal_to"
    | "contains"
    | "does_not_contain"
    | "is_present"
    | "is_not_present";
  values: string[];
};

type Props = {
  hasActiveFilters: boolean;
  onApplyFilters: (filters: FilterCondition[]) => void;
  onClearFilters: () => void;
};

const ATTRIBUTE_OPTIONS = [
  { value: "name", label: "Name" },
  { value: "email", label: "Email" },
  { value: "phone_number", label: "Phone number" },
  { value: "identifier", label: "Identifier" },
];

const OPERATOR_OPTIONS = [
  { value: "contains", label: "contains" },
  { value: "does_not_contain", label: "does not contain" },
  { value: "equal_to", label: "is equal to" },
  { value: "not_equal_to", label: "is not equal to" },
  { value: "is_present", label: "is present" },
  { value: "is_not_present", label: "is not present" },
];

export default function ContactFilter({
  hasActiveFilters,
  onApplyFilters,
  onClearFilters,
}: Props) {
  const [open, setOpen] = useState(false);
  const [conditions, setConditions] = useState<FilterCondition[]>([
    { attribute_key: "name", filter_operator: "contains", values: [""] },
  ]);
  const panelRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    function handleClickOutside(e: MouseEvent) {
      if (panelRef.current && !panelRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    }
    if (open) {
      document.addEventListener("mousedown", handleClickOutside);
    }
    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
    };
  }, [open]);

  const addCondition = () => {
    setConditions((prev) => [
      ...prev,
      { attribute_key: "name", filter_operator: "contains", values: [""] },
    ]);
  };

  const removeCondition = (index: number) => {
    setConditions((prev) => {
      const next = prev.filter((_, i) => i !== index);
      return next.length > 0
        ? next
        : [{ attribute_key: "name", filter_operator: "contains", values: [""] }];
    });
  };

  const updateCondition = (
    index: number,
    field: "attribute_key" | "filter_operator" | "value",
    val: string
  ) => {
    setConditions((prev) =>
      prev.map((c, i) => {
        if (i !== index) return c;
        if (field === "value") {
          return { ...c, values: [val] };
        }
        return { ...c, [field]: val };
      })
    );
  };

  const apply = () => {
    const valid = conditions.filter(
      (c) =>
        c.filter_operator === "is_present" ||
        c.filter_operator === "is_not_present" ||
        (c.values[0] && c.values[0].trim().length > 0)
    );
    onApplyFilters(valid);
    setOpen(false);
  };

  const clear = () => {
    setConditions([
      { attribute_key: "name", filter_operator: "contains", values: [""] },
    ]);
    onClearFilters();
    setOpen(false);
  };

  return (
    <div ref={panelRef} className="relative">
      <Button
        id="toggleContactsFilterButton"
        icon="i-lucide-list-filter"
        color="slate"
        size="sm"
        variant="ghost"
        className={`relative ${open ? "bg-n-alpha-2" : ""}`}
        onClick={() => setOpen(!open)}
        aria-label="Filter contacts"
      >
        {hasActiveFilters && (
          <div className="absolute top-1 right-1 w-2 h-2 rounded-full bg-n-brand" />
        )}
      </Button>

      {open && (
        <div className="absolute top-full mt-1.5 ltr:right-0 rtl:left-0 z-50 w-[min(34rem,calc(100vw-2rem))] overflow-visible border border-n-weak bg-n-alpha-3 backdrop-blur-[100px] shadow-xl rounded-xl p-5 flex flex-col gap-4">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-n-slate-12">
              {t("CONTACTS_LAYOUT.FILTER.TITLE")}
            </h3>
            {hasActiveFilters && (
              <span className="text-xs text-n-brand font-medium">Filtered</span>
            )}
          </div>

          <div className="flex flex-col gap-3 max-h-72 overflow-y-auto">
            {conditions.map((cond, index) => {
              const noValueNeeded =
                cond.filter_operator === "is_present" ||
                cond.filter_operator === "is_not_present";
              return (
                <div
                  key={index}
                  className="flex items-center gap-2 rounded-lg border border-n-weak bg-n-surface-1 p-2.5"
                >
                  <select
                    value={cond.attribute_key}
                    onChange={(e) =>
                      updateCondition(index, "attribute_key", e.target.value)
                    }
                    className="text-xs rounded border border-n-strong bg-n-surface-1 px-2 py-1 text-n-slate-12 outline-none focus:border-n-brand"
                  >
                    {ATTRIBUTE_OPTIONS.map((opt) => (
                      <option key={opt.value} value={opt.value}>
                        {opt.label}
                      </option>
                    ))}
                  </select>

                  <select
                    value={cond.filter_operator}
                    onChange={(e) =>
                      updateCondition(index, "filter_operator", e.target.value)
                    }
                    className="text-xs rounded border border-n-strong bg-n-surface-1 px-2 py-1 text-n-slate-12 outline-none focus:border-n-brand"
                  >
                    {OPERATOR_OPTIONS.map((opt) => (
                      <option key={opt.value} value={opt.value}>
                        {opt.label}
                      </option>
                    ))}
                  </select>

                  {!noValueNeeded && (
                    <input
                      type="text"
                      placeholder="Value"
                      value={cond.values[0] ?? ""}
                      onChange={(e) =>
                        updateCondition(index, "value", e.target.value)
                      }
                      className="min-w-0 flex-1 text-xs rounded border border-n-strong bg-n-surface-1 px-2 py-1 text-n-slate-12 placeholder:text-n-slate-10 outline-none focus:border-n-brand"
                    />
                  )}

                  {conditions.length > 1 && (
                    <button
                      type="button"
                      onClick={() => removeCondition(index)}
                      className="p-1 text-n-slate-10 hover:text-n-ruby-11 transition-colors"
                      aria-label="Remove filter"
                    >
                      <span className="text-base leading-none">&times;</span>
                    </button>
                  )}
                </div>
              );
            })}
          </div>

          <div className="flex items-center justify-between pt-2 border-t border-n-weak">
            <Button
              variant="ghost"
              color="blue"
              size="sm"
              onClick={addCondition}
              label={t("CONTACTS_LAYOUT.FILTER.BUTTONS.ADD_FILTER")}
            />
            <div className="flex items-center gap-2">
              <Button
                variant="faded"
                color="slate"
                size="sm"
                onClick={clear}
                label={t("CONTACTS_LAYOUT.FILTER.BUTTONS.CLEAR_FILTERS")}
              />
              <Button
                variant="solid"
                color="blue"
                size="sm"
                onClick={apply}
                label={t("CONTACTS_LAYOUT.FILTER.BUTTONS.APPLY_FILTERS")}
              />
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
