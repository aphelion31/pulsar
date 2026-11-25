import { useState, useMemo } from "react";
import Avatar from "~/components/next/Avatar";
import Button from "~/components/next/Button";
import Input from "~/components/next/Input";
import TextArea from "~/components/next/TextArea";
import { timeAgo } from "~/lib/format";
import { t } from "~/lib/i18n";
import type { ContactItem } from "./types";

type Props = {
  contact: ContactItem;
  onUpdate: (updated: Partial<ContactItem>) => Promise<void>;
  onDelete: (contact: ContactItem) => void;
  onViewDetails: (id: number) => void;
  isUpdating?: boolean;
};

export default function ContactsCard({
  contact,
  onUpdate,
  onDelete,
  onViewDetails,
  isUpdating = false,
}: Props) {
  const [expanded, setExpanded] = useState(false);

  // Split name
  const nameParts = (contact.name ?? "").trim().split(/\s+/);
  const initialFirstName = nameParts[0] ?? "";
  const initialLastName = nameParts.slice(1).join(" ");

  const [firstName, setFirstName] = useState(initialFirstName);
  const [lastName, setLastName] = useState(initialLastName);
  const [email, setEmail] = useState(contact.email ?? "");
  const [phone, setPhone] = useState(contact.phone_number ?? "");

  const addAttrs = (contact.additional_attributes ?? {}) as Record<string, any>;
  const [companyName, setCompanyName] = useState(addAttrs.company_name ?? addAttrs.companyName ?? "");
  const [city, setCity] = useState(addAttrs.city ?? "");
  const [country, setCountry] = useState(addAttrs.country ?? addAttrs.countryCode ?? "");
  const [bio, setBio] = useState(addAttrs.description ?? "");

  const soc = (addAttrs.social_profiles ?? addAttrs.socialProfiles ?? {}) as Record<string, string>;
  const [twitter, setTwitter] = useState(soc.twitter ?? "");
  const [linkedin, setLinkedin] = useState(soc.linkedin ?? "");
  const [github, setGithub] = useState(soc.github ?? "");
  const [facebook, setFacebook] = useState(soc.facebook ?? "");

  const [feedback, setFeedback] = useState<{ type: "success" | "error"; message: string } | null>(null);
  const [localSaving, setLocalSaving] = useState(false);

  const locationText = useMemo(() => {
    return [city, country].filter(Boolean).join(", ");
  }, [city, country]);

  const handleUpdate = async () => {
    const fullName = `${firstName.trim()} ${lastName.trim()}`.trim();
    if (!fullName) {
      setFeedback({ type: "error", message: "Name cannot be empty" });
      return;
    }

    setLocalSaving(true);
    setFeedback(null);

    const socialProfiles: Record<string, string> = {};
    if (twitter.trim()) socialProfiles.twitter = twitter.trim();
    if (linkedin.trim()) socialProfiles.linkedin = linkedin.trim();
    if (github.trim()) socialProfiles.github = github.trim();
    if (facebook.trim()) socialProfiles.facebook = facebook.trim();

    const newAddAttrs = {
      ...addAttrs,
      description: bio.trim() || undefined,
      company_name: companyName.trim() || undefined,
      city: city.trim() || undefined,
      country: country.trim() || undefined,
      social_profiles: Object.keys(socialProfiles).length ? socialProfiles : undefined,
    };

    try {
      await onUpdate({
        id: contact.id,
        name: fullName,
        email: email.trim() || null,
        phone_number: phone.trim() || null,
        additional_attributes: newAddAttrs,
      });
      setFeedback({
        type: "success",
        message: t("CONTACTS_LAYOUT.CARD.EDIT_DETAILS_FORM.SUCCESS_MESSAGE"),
      });
    } catch (err: unknown) {
      setFeedback({
        type: "error",
        message: err instanceof Error ? err.message : t("CONTACTS_LAYOUT.CARD.EDIT_DETAILS_FORM.ERROR_MESSAGE"),
      });
    } finally {
      setLocalSaving(false);
    }
  };

  return (
    <div className="flex flex-col w-full outline outline-1 outline-n-container -outline-offset-1 rounded-xl bg-n-solid-2 transition-all duration-200">
      <div className="flex w-full items-center justify-between gap-3 px-6 py-4">
        <div className="flex items-center justify-start flex-1 min-w-0 gap-4">
          <Avatar
            name={contact.name || "Unknown"}
            src={contact.thumbnail || ""}
            size={42}
            hideOfflineStatus
          />
          <div className="flex flex-col gap-0.5 flex-1 min-w-0">
            <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
              <span className="text-base font-medium truncate text-n-slate-12">
                {contact.name || "Unknown contact"}
              </span>
              {companyName && (
                <span className="inline-flex items-center gap-1 text-sm text-n-slate-11 truncate">
                  <span className="text-n-slate-10 text-xs">at</span>
                  {companyName}
                </span>
              )}
            </div>
            <div className="flex flex-wrap items-center justify-start gap-x-2.5 gap-y-1 text-sm text-n-slate-11">
              {contact.email && (
                <span className="truncate max-w-64" title={contact.email}>
                  {contact.email}
                </span>
              )}
              {contact.email && contact.phone_number && (
                <span className="w-px h-3 bg-n-slate-6 shrink-0" />
              )}
              {contact.phone_number && (
                <span className="truncate">{contact.phone_number}</span>
              )}
              {locationText && (contact.email || contact.phone_number) && (
                <span className="w-px h-3 bg-n-slate-6 shrink-0" />
              )}
              {locationText && <span className="truncate">{locationText}</span>}
              <span className="w-px h-3 bg-n-slate-6 shrink-0" />
              <Button
                label={t("CONTACTS_LAYOUT.CARD.VIEW_DETAILS")}
                variant="link"
                color="blue"
                size="xs"
                className="p-0 text-xs font-medium"
                onClick={() => onViewDetails(contact.id)}
              />
            </div>
          </div>
        </div>

        <div className="flex items-center gap-3 shrink-0">
          {contact.last_activity_at && (
            <span className="hidden sm:inline-block text-xs tabular-nums text-n-slate-10">
              {timeAgo(contact.last_activity_at)}
            </span>
          )}
          <Button
            icon="i-lucide-chevron-down"
            variant="ghost"
            color="slate"
            size="sm"
            className={`transition-transform duration-200 ${expanded ? "rotate-180" : ""}`}
            onClick={() => setExpanded(!expanded)}
            aria-label="Expand contact form"
          />
        </div>
      </div>

      {expanded && (
        <div className="border-t border-n-strong p-6 flex flex-col gap-6 animate-in fade-in duration-150">
          {feedback && (
            <div
              className={`rounded-lg p-3 text-sm ${
                feedback.type === "success"
                  ? "bg-n-teal-9/10 border border-n-teal-8/30 text-n-teal-11"
                  : "bg-n-ruby-9/10 border border-n-ruby-8/30 text-n-ruby-11"
              }`}
            >
              {feedback.message}
            </div>
          )}

          <div className="flex flex-col gap-4">
            <span className="text-sm font-medium text-n-slate-12">
              {t("CONTACTS_LAYOUT.CARD.EDIT_DETAILS_FORM.TITLE")}
            </span>
            <div className="grid w-full grid-cols-1 sm:grid-cols-2 gap-4">
              <Input
                label="First Name"
                placeholder="First name"
                modelValue={firstName}
                onModelValueChange={(v) => setFirstName(String(v))}
              />
              <Input
                label="Last Name"
                placeholder="Last name"
                modelValue={lastName}
                onModelValueChange={(v) => setLastName(String(v))}
              />
              <Input
                label="Email Address"
                placeholder="Enter email"
                type="email"
                modelValue={email}
                onModelValueChange={(v) => setEmail(String(v))}
              />
              <Input
                label="Phone Number"
                placeholder="Enter phone"
                type="tel"
                modelValue={phone}
                onModelValueChange={(v) => setPhone(String(v))}
              />
              <Input
                label="Company Name"
                placeholder="Company"
                modelValue={companyName}
                onModelValueChange={(v) => setCompanyName(String(v))}
              />
              <Input
                label="City"
                placeholder="City"
                modelValue={city}
                onModelValueChange={(v) => setCity(String(v))}
              />
              <Input
                label="Country"
                placeholder="Country"
                modelValue={country}
                onModelValueChange={(v) => setCountry(String(v))}
              />
            </div>
            <TextArea
              label="Bio"
              placeholder="Enter biography"
              modelValue={bio}
              onModelValueChange={(v) => setBio(v)}
              autoHeight
            />
          </div>

          <div className="flex flex-col gap-3">
            <span className="text-sm font-medium text-n-slate-12">
              {t("CONTACTS_LAYOUT.CARD.SOCIAL_MEDIA.TITLE")}
            </span>
            <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
              <Input
                label="Twitter"
                placeholder="username"
                modelValue={twitter}
                onModelValueChange={(v) => setTwitter(String(v))}
              />
              <Input
                label="LinkedIn"
                placeholder="username"
                modelValue={linkedin}
                onModelValueChange={(v) => setLinkedin(String(v))}
              />
              <Input
                label="GitHub"
                placeholder="username"
                modelValue={github}
                onModelValueChange={(v) => setGithub(String(v))}
              />
              <Input
                label="Facebook"
                placeholder="username"
                modelValue={facebook}
                onModelValueChange={(v) => setFacebook(String(v))}
              />
            </div>
          </div>

          <div className="flex items-center justify-between pt-4 border-t border-n-strong">
            <Button
              type="button"
              variant="solid"
              color="blue"
              size="sm"
              label={t("CONTACTS_LAYOUT.CARD.EDIT_DETAILS_FORM.UPDATE_BUTTON")}
              isLoading={localSaving || isUpdating}
              disabled={localSaving || isUpdating || !firstName.trim()}
              onClick={handleUpdate}
            />
            <Button
              type="button"
              variant="outline"
              color="ruby"
              size="sm"
              label={t("CONTACTS_LAYOUT.DETAILS.DELETE_CONTACT")}
              onClick={() => onDelete(contact)}
            />
          </div>
        </div>
      )}
    </div>
  );
}
