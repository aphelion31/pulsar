import { useState } from "react";
import Button from "~/components/next/Button";
import Input from "~/components/next/Input";
import TextArea from "~/components/next/TextArea";
import { t } from "~/lib/i18n";
import type { ContactItem } from "./types";

type Props = {
  open: boolean;
  onClose: () => void;
  onCreate: (contact: Partial<ContactItem>) => Promise<void>;
  isLoading?: boolean;
};

export default function CreateContactDialog({
  open,
  onClose,
  onCreate,
  isLoading = false,
}: Props) {
  const [firstName, setFirstName] = useState("");
  const [lastName, setLastName] = useState("");
  const [email, setEmail] = useState("");
  const [phone, setPhone] = useState("");
  const [companyName, setCompanyName] = useState("");
  const [city, setCity] = useState("");
  const [country, setCountry] = useState("");
  const [bio, setBio] = useState("");
  const [twitter, setTwitter] = useState("");
  const [linkedin, setLinkedin] = useState("");
  const [github, setGithub] = useState("");
  const [facebook, setFacebook] = useState("");
  const [error, setError] = useState("");

  if (!open) return null;

  const reset = () => {
    setFirstName("");
    setLastName("");
    setEmail("");
    setPhone("");
    setCompanyName("");
    setCity("");
    setCountry("");
    setBio("");
    setTwitter("");
    setLinkedin("");
    setGithub("");
    setFacebook("");
    setError("");
  };

  const handleClose = () => {
    reset();
    onClose();
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    const fullName = `${firstName.trim()} ${lastName.trim()}`.trim();
    if (!fullName) {
      setError("Name is required");
      return;
    }

    setError("");
    const socialProfiles: Record<string, string> = {};
    if (twitter.trim()) socialProfiles.twitter = twitter.trim();
    if (linkedin.trim()) socialProfiles.linkedin = linkedin.trim();
    if (github.trim()) socialProfiles.github = github.trim();
    if (facebook.trim()) socialProfiles.facebook = facebook.trim();

    const additionalAttributes: Record<string, unknown> = {
      description: bio.trim() || undefined,
      company_name: companyName.trim() || undefined,
      city: city.trim() || undefined,
      country: country.trim() || undefined,
      social_profiles: Object.keys(socialProfiles).length ? socialProfiles : undefined,
    };

    try {
      await onCreate({
        name: fullName,
        email: email.trim() || null,
        phone_number: phone.trim() || null,
        additional_attributes: additionalAttributes,
      });
      handleClose();
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Failed to create contact");
    }
  };

  return (
    <div
      role="dialog"
      aria-modal="true"
      className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/40 backdrop-blur-xs animate-in fade-in duration-200"
    >
      <div className="relative w-full max-w-2xl max-h-[90vh] overflow-y-auto rounded-2xl border border-n-weak bg-n-surface-1 p-6 shadow-2xl flex flex-col gap-6">
        <div className="flex items-center justify-between border-b border-n-weak pb-4">
          <h2 className="text-lg font-medium text-n-slate-12">
            {t("CONTACTS_LAYOUT.HEADER.ACTIONS.CONTACT_CREATION.ADD_CONTACT")}
          </h2>
          <button
            type="button"
            onClick={handleClose}
            className="text-n-slate-10 hover:text-n-slate-12 transition-colors p-1"
            aria-label="Close"
          >
            <span className="text-xl leading-none">&times;</span>
          </button>
        </div>

        {error && (
          <div className="rounded-lg bg-n-ruby-9/10 border border-n-ruby-8/30 p-3 text-sm text-n-ruby-11">
            {error}
          </div>
        )}

        <form onSubmit={handleSubmit} className="flex flex-col gap-6">
          <div className="flex flex-col gap-4">
            <h3 className="text-sm font-medium text-n-slate-12">
              {t("CONTACTS_LAYOUT.CARD.EDIT_DETAILS_FORM.TITLE")}
            </h3>
            <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
              <Input
                label="First Name"
                placeholder="First name"
                modelValue={firstName}
                onModelValueChange={(v) => setFirstName(String(v))}
                required
              />
              <Input
                label="Last Name"
                placeholder="Last name"
                modelValue={lastName}
                onModelValueChange={(v) => setLastName(String(v))}
              />
              <Input
                label="Email Address"
                placeholder="Enter email address"
                type="email"
                modelValue={email}
                onModelValueChange={(v) => setEmail(String(v))}
              />
              <Input
                label="Phone Number"
                placeholder="Enter phone number"
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
              placeholder="Enter biography or note"
              modelValue={bio}
              onModelValueChange={(v) => setBio(v)}
              autoHeight
            />
          </div>

          <div className="flex flex-col gap-3">
            <h3 className="text-sm font-medium text-n-slate-12">
              {t("CONTACTS_LAYOUT.CARD.SOCIAL_MEDIA.TITLE")}
            </h3>
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

          <div className="flex items-center justify-between pt-4 border-t border-n-weak">
            <Button
              type="button"
              variant="link"
              color="slate"
              label={t("DIALOG.BUTTONS.CANCEL")}
              onClick={handleClose}
            />
            <Button
              type="submit"
              variant="solid"
              color="blue"
              isLoading={isLoading}
              disabled={isLoading || !firstName.trim()}
              label={t("CONTACTS_LAYOUT.HEADER.ACTIONS.CONTACT_CREATION.SAVE_CONTACT")}
            />
          </div>
        </form>
      </div>
    </div>
  );
}
