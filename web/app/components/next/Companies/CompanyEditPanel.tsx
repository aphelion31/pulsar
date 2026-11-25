import { useState, useEffect } from "react";
import Button from "~/components/next/Button";
import Input from "~/components/next/Input";
import TextArea from "~/components/next/TextArea";
import { t } from "~/lib/i18n";
import type { CompanyItem } from "./types";

type Props = {
  open: boolean;
  company: CompanyItem;
  onClose: () => void;
  onSave: (payload: Record<string, unknown>) => Promise<void>;
  isLoading?: boolean;
};

export default function CompanyEditPanel({
  open,
  company,
  onClose,
  onSave,
  isLoading = false,
}: Props) {
  const [name, setName] = useState("");
  const [domain, setDomain] = useState("");
  const [description, setDescription] = useState("");
  const [industry, setIndustry] = useState("");
  const [employees, setEmployees] = useState("");
  const [city, setCity] = useState("");
  const [country, setCountry] = useState("");
  const [phone, setPhone] = useState("");

  const [linkedin, setLinkedin] = useState("");
  const [xHandle, setXHandle] = useState("");
  const [facebook, setFacebook] = useState("");
  const [instagram, setInstagram] = useState("");
  const [github, setGithub] = useState("");
  const [youtube, setYoutube] = useState("");
  const [error, setError] = useState("");

  useEffect(() => {
    if (!open) return;
    const attrs = (company.additional_attributes ?? {}) as Record<string, any>;
    const soc = (attrs.social_profiles ?? {}) as Record<string, string>;

    setName(company.name || "");
    setDomain(company.domain || "");
    setDescription(company.description || "");
    setIndustry(attrs.sub_industry || attrs.industry || "");
    setEmployees(String(attrs.employee_count || attrs.employee_count_range || ""));
    setCity(attrs.city || "");
    setCountry(attrs.country || "");
    setPhone(attrs.phone || "");

    setLinkedin(soc.linkedin || "");
    setXHandle(soc.x || soc.twitter || "");
    setFacebook(soc.facebook || "");
    setInstagram(soc.instagram || "");
    setGithub(soc.github || "");
    setYoutube(soc.youtube || "");
    setError("");
  }, [open, company]);

  if (!open) return null;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim()) {
      setError("Company name is required");
      return;
    }

    setError("");

    const attrs = { ...(company.additional_attributes ?? {}) } as Record<string, any>;
    if (industry.trim()) attrs.industry = industry.trim();
    else delete attrs.industry;
    delete attrs.sub_industry;

    if (employees.trim()) attrs.employee_count_range = employees.trim();
    else delete attrs.employee_count_range;
    delete attrs.employee_count;

    if (city.trim()) attrs.city = city.trim();
    else delete attrs.city;

    if (country.trim()) attrs.country = country.trim();
    else delete attrs.country;

    if (phone.trim()) attrs.phone = phone.trim();
    else delete attrs.phone;

    const socialProfiles: Record<string, string> = {};
    if (linkedin.trim()) socialProfiles.linkedin = linkedin.trim();
    if (xHandle.trim()) socialProfiles.x = xHandle.trim();
    if (facebook.trim()) socialProfiles.facebook = facebook.trim();
    if (instagram.trim()) socialProfiles.instagram = instagram.trim();
    if (github.trim()) socialProfiles.github = github.trim();
    if (youtube.trim()) socialProfiles.youtube = youtube.trim();

    if (Object.keys(socialProfiles).length > 0) {
      attrs.social_profiles = socialProfiles;
    } else {
      delete attrs.social_profiles;
    }

    try {
      await onSave({
        name: name.trim(),
        domain: domain.trim() || null,
        description: description.trim() || null,
        additional_attributes: attrs,
      });
      onClose();
    } catch (err: unknown) {
      setError(
        err instanceof Error ? err.message : t("COMPANIES.DETAIL.PROFILE.MESSAGES.UPDATE_ERROR")
      );
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex justify-end bg-black/40 backdrop-blur-xs animate-in fade-in duration-200">
      <div className="w-full max-w-xl h-full bg-n-surface-1 border-l border-n-weak shadow-2xl flex flex-col">
        <header className="flex items-center justify-between px-6 py-4 border-b border-n-weak shrink-0">
          <h2 className="text-lg font-medium text-n-slate-12">
            {t("COMPANIES.DETAIL.EDIT.TITLE")}
          </h2>
          <button
            type="button"
            onClick={onClose}
            className="text-n-slate-10 hover:text-n-slate-12 p-1"
            aria-label="Close"
          >
            <span className="text-xl leading-none">&times;</span>
          </button>
        </header>

        <form onSubmit={handleSubmit} className="flex flex-col flex-1 min-h-0">
          <div className="flex-1 overflow-y-auto px-6 py-6 flex flex-col gap-6">
            {error && (
              <div className="rounded-lg bg-n-ruby-9/10 border border-n-ruby-8/30 p-3 text-sm text-n-ruby-11">
                {error}
              </div>
            )}

            <section className="flex flex-col gap-4">
              <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
                <Input
                  label={t("COMPANIES.DETAIL.PROFILE.FIELDS.NAME")}
                  modelValue={name}
                  onModelValueChange={(v) => setName(String(v))}
                  required
                />
                <Input
                  label={t("COMPANIES.DETAIL.PROFILE.FIELDS.DOMAIN")}
                  placeholder={t("COMPANIES.DETAIL.EDIT.PLACEHOLDERS.DOMAIN")}
                  modelValue={domain}
                  onModelValueChange={(v) => setDomain(String(v))}
                />
              </div>
              <TextArea
                label={t("COMPANIES.DETAIL.EDIT.DESCRIPTION")}
                placeholder={t("COMPANIES.DETAIL.PROFILE.DESCRIPTION_PLACEHOLDER")}
                modelValue={description}
                onModelValueChange={(v) => setDescription(v)}
                maxLength={1000}
                showCharacterCount
                autoHeight
              />
            </section>

            <section className="flex flex-col gap-4 pt-4 border-t border-n-weak">
              <h3 className="text-sm font-medium text-n-slate-12">
                {t("COMPANIES.DETAIL.EDIT.DETAILS")}
              </h3>
              <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
                <Input
                  label={t("COMPANIES.DETAIL.FACTS.INDUSTRY")}
                  placeholder={t("COMPANIES.DETAIL.EDIT.PLACEHOLDERS.INDUSTRY")}
                  modelValue={industry}
                  onModelValueChange={(v) => setIndustry(String(v))}
                />
                <Input
                  label={t("COMPANIES.DETAIL.FACTS.SIZE")}
                  placeholder={t("COMPANIES.DETAIL.EDIT.PLACEHOLDERS.EMPLOYEES")}
                  modelValue={employees}
                  onModelValueChange={(v) => setEmployees(String(v))}
                />
                <Input
                  label={t("COMPANIES.DETAIL.EDIT.CITY")}
                  modelValue={city}
                  onModelValueChange={(v) => setCity(String(v))}
                />
                <Input
                  label={t("COMPANIES.DETAIL.EDIT.COUNTRY")}
                  modelValue={country}
                  onModelValueChange={(v) => setCountry(String(v))}
                />
                <Input
                  label={t("COMPANIES.DETAIL.EDIT.PHONE")}
                  type="tel"
                  modelValue={phone}
                  onModelValueChange={(v) => setPhone(String(v))}
                />
              </div>
            </section>

            <section className="flex flex-col gap-4 pt-4 border-t border-n-weak">
              <h3 className="text-sm font-medium text-n-slate-12">
                {t("COMPANIES.DETAIL.EDIT.SOCIAL.TITLE")}
              </h3>
              <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
                <Input
                  label={t("COMPANIES.DETAIL.EDIT.SOCIAL.LINKEDIN")}
                  placeholder="https://linkedin.com/company/..."
                  modelValue={linkedin}
                  onModelValueChange={(v) => setLinkedin(String(v))}
                />
                <Input
                  label={t("COMPANIES.DETAIL.EDIT.SOCIAL.X")}
                  placeholder="https://x.com/..."
                  modelValue={xHandle}
                  onModelValueChange={(v) => setXHandle(String(v))}
                />
                <Input
                  label={t("COMPANIES.DETAIL.EDIT.SOCIAL.FACEBOOK")}
                  placeholder="https://facebook.com/..."
                  modelValue={facebook}
                  onModelValueChange={(v) => setFacebook(String(v))}
                />
                <Input
                  label={t("COMPANIES.DETAIL.EDIT.SOCIAL.INSTAGRAM")}
                  placeholder="https://instagram.com/..."
                  modelValue={instagram}
                  onModelValueChange={(v) => setInstagram(String(v))}
                />
                <Input
                  label={t("COMPANIES.DETAIL.EDIT.SOCIAL.GITHUB")}
                  placeholder="https://github.com/..."
                  modelValue={github}
                  onModelValueChange={(v) => setGithub(String(v))}
                />
                <Input
                  label={t("COMPANIES.DETAIL.EDIT.SOCIAL.YOUTUBE")}
                  placeholder="https://youtube.com/..."
                  modelValue={youtube}
                  onModelValueChange={(v) => setYoutube(String(v))}
                />
              </div>
            </section>
          </div>

          <footer className="flex items-center justify-end gap-3 px-6 py-4 border-t border-n-weak shrink-0 bg-n-surface-1">
            <Button
              type="button"
              variant="faded"
              color="slate"
              label={t("DIALOG.BUTTONS.CANCEL")}
              onClick={onClose}
              disabled={isLoading}
            />
            <Button
              type="submit"
              variant="solid"
              color="blue"
              label={t("COMPANIES.DETAIL.EDIT.SAVE")}
              isLoading={isLoading}
              disabled={isLoading || !name.trim()}
            />
          </footer>
        </form>
      </div>
    </div>
  );
}
