import { useState, useEffect, useCallback, useMemo } from "react";
import { useParams, useNavigate, useOutletContext, Link } from "react-router";
import { acct, api } from "~/lib/api";
import { timeAgo } from "~/lib/format";
import { t } from "~/lib/i18n";
import Avatar from "~/components/next/Avatar";
import Button from "~/components/next/Button";
import Input from "~/components/next/Input";
import TextArea from "~/components/next/TextArea";
import Spinner from "~/components/next/Spinner";
import TabBar from "~/components/next/TabBar";
import ContactNotes from "~/components/next/Contacts/ContactNotes";
import ContactConversations from "~/components/next/Contacts/ContactConversations";
import ContactMergeDialog from "~/components/next/Contacts/ContactMergeDialog";
import ConfirmContactDeleteDialog from "~/components/next/Contacts/ConfirmContactDeleteDialog";
import type { ContactItem } from "~/components/next/Contacts/types";
import type { ShellCtx } from "./shell";

export default function ContactDetailPage() {
  const { accountId, me } = useOutletContext<ShellCtx>();
  const params = useParams();
  const navigate = useNavigate();

  const contactId = Number(params.contactId);

  const [contact, setContact] = useState<ContactItem | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState("");
  const [revision, setRevision] = useState(0);

  // Profile form state
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

  const [isUpdating, setIsUpdating] = useState(false);
  const [updateFeedback, setUpdateFeedback] = useState<{
    type: "success" | "error";
    message: string;
  } | null>(null);

  // Tabs: notes, conversations, merge
  const [activeTabKey, setActiveTabKey] = useState<"notes" | "conversations" | "merge">("notes");
  const [mergeDialogOpen, setMergeDialogOpen] = useState(false);
  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);
  const [isDeleting, setIsDeleting] = useState(false);
  const [isBlocking, setIsBlocking] = useState(false);

  const base = `${acct(accountId)}/contacts/${contactId}`;

  const fetchContact = useCallback(async () => {
    setIsLoading(true);
    setError("");
    try {
      const res = await api<{ payload: ContactItem }>(base);
      const c = res.payload;
      setContact(c);

      const parts = (c.name ?? "").trim().split(/\s+/);
      setFirstName(parts[0] ?? "");
      setLastName(parts.slice(1).join(" "));
      setEmail(c.email ?? "");
      setPhone(c.phone_number ?? "");

      const addAttrs = (c.additional_attributes ?? {}) as Record<string, any>;
      setCompanyName(addAttrs.company_name ?? addAttrs.companyName ?? "");
      setCity(addAttrs.city ?? "");
      setCountry(addAttrs.country ?? addAttrs.countryCode ?? "");
      setBio(addAttrs.description ?? "");

      const soc = (addAttrs.social_profiles ?? addAttrs.socialProfiles ?? {}) as Record<
        string,
        string
      >;
      setTwitter(soc.twitter ?? "");
      setLinkedin(soc.linkedin ?? "");
      setGithub(soc.github ?? "");
      setFacebook(soc.facebook ?? "");
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Failed to load contact");
    } finally {
      setIsLoading(false);
    }
  }, [base]);

  useEffect(() => {
    if (contactId) {
      fetchContact();
    }
  }, [contactId, fetchContact, revision]);

  const handleUpdate = async (e?: React.FormEvent) => {
    if (e) e.preventDefault();
    const fullName = `${firstName.trim()} ${lastName.trim()}`.trim();
    if (!fullName) {
      setUpdateFeedback({ type: "error", message: "Name is required" });
      return;
    }

    setIsUpdating(true);
    setUpdateFeedback(null);

    const socialProfiles: Record<string, string> = {};
    if (twitter.trim()) socialProfiles.twitter = twitter.trim();
    if (linkedin.trim()) socialProfiles.linkedin = linkedin.trim();
    if (github.trim()) socialProfiles.github = github.trim();
    if (facebook.trim()) socialProfiles.facebook = facebook.trim();

    const additionalAttributes = {
      ...(contact?.additional_attributes ?? {}),
      description: bio.trim() || undefined,
      company_name: companyName.trim() || undefined,
      city: city.trim() || undefined,
      country: country.trim() || undefined,
      social_profiles: Object.keys(socialProfiles).length ? socialProfiles : undefined,
    };

    try {
      const res = await api<{ payload: ContactItem }>(base, {
        method: "PATCH",
        json: {
          name: fullName,
          email: email.trim() || null,
          phone_number: phone.trim() || null,
          additional_attributes: additionalAttributes,
        },
      });
      setContact(res.payload);
      setUpdateFeedback({
        type: "success",
        message: t("CONTACTS_LAYOUT.CARD.EDIT_DETAILS_FORM.SUCCESS_MESSAGE"),
      });
    } catch (err: unknown) {
      setUpdateFeedback({
        type: "error",
        message:
          err instanceof Error
            ? err.message
            : t("CONTACTS_LAYOUT.CARD.EDIT_DETAILS_FORM.ERROR_MESSAGE"),
      });
    } finally {
      setIsUpdating(false);
    }
  };

  const handleToggleBlock = async () => {
    if (!contact) return;
    setIsBlocking(true);
    try {
      const nextBlocked = !contact.blocked;
      const res = await api<{ payload: ContactItem }>(base, {
        method: "PATCH",
        json: { blocked: nextBlocked },
      });
      setContact(res.payload);
    } catch (err: unknown) {
      alert(err instanceof Error ? err.message : "Failed to toggle block");
    } finally {
      setIsBlocking(false);
    }
  };

  const handleDelete = async () => {
    if (!contact) return;
    setIsDeleting(true);
    try {
      await api(base, { method: "DELETE" });
      navigate(`/accounts/${accountId}/contacts`);
    } catch (err: unknown) {
      alert(err instanceof Error ? err.message : "Failed to delete contact");
      setIsDeleting(false);
    }
  };

  const tabs = useMemo(
    () => [
      { key: "notes", label: t("CONTACTS_LAYOUT.SIDEBAR.TABS.NOTES") },
      { key: "conversations", label: t("CONTACTS_LAYOUT.SIDEBAR.TABS.HISTORY") },
      { key: "merge", label: t("CONTACTS_LAYOUT.SIDEBAR.TABS.MERGE") },
    ],
    []
  );

  const activeTabIndex = tabs.findIndex((t) => t.key === activeTabKey);

  if (isLoading) {
    return (
      <div className="flex h-full w-full items-center justify-center bg-n-surface-1">
        <Spinner size={32} />
      </div>
    );
  }

  if (error || !contact) {
    return (
      <div className="flex flex-col h-full w-full items-center justify-center gap-4 bg-n-surface-1 p-6 text-center">
        <p className="text-base font-medium text-n-slate-12">
          {error || "Contact not found"}
        </p>
        <Button
          variant="solid"
          color="blue"
          label="Back to Contacts"
          onClick={() => navigate(`/accounts/${accountId}/contacts`)}
        />
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full w-full min-w-0 bg-n-surface-1 overflow-hidden">
      {/* Top Header / Breadcrumbs */}
      <header className="sticky top-0 z-20 px-6 bg-n-surface-1/90 backdrop-blur-md border-b border-n-weak">
        <div className="flex items-center justify-between w-full py-4 gap-4 max-w-7xl mx-auto">
          <div className="flex items-center gap-2 min-w-0">
            <Link
              to={`/accounts/${accountId}/contacts`}
              className="text-sm font-medium text-n-slate-11 hover:text-n-slate-12 transition-colors truncate"
            >
              {t("CONTACTS_LAYOUT.HEADER.BREADCRUMB.CONTACTS")}
            </Link>
            <span className="text-n-slate-8">&gt;</span>
            <span className="text-sm font-medium text-n-slate-12 truncate">
              {contact.name || "Unknown"}
            </span>
          </div>

          <div className="flex items-center gap-2 shrink-0">
            <Button
              variant="outline"
              color={contact.blocked ? "ruby" : "slate"}
              size="sm"
              isLoading={isBlocking}
              disabled={isBlocking}
              label={
                contact.blocked
                  ? t("CONTACTS_LAYOUT.HEADER.UNBLOCK_CONTACT")
                  : t("CONTACTS_LAYOUT.HEADER.BLOCK_CONTACT")
              }
              onClick={handleToggleBlock}
            />
            <Button
              variant="solid"
              color="blue"
              size="sm"
              label={t("CONTACTS_LAYOUT.HEADER.SEND_MESSAGE")}
              onClick={() => {
                navigate(`/accounts/${accountId}/conversations`);
              }}
            />
          </div>
        </div>
      </header>

      {/* Main split content */}
      <div className="flex flex-1 min-h-0 overflow-hidden max-w-7xl w-full mx-auto">
        {/* Left column: Profile form & Details */}
        <main className="flex-1 overflow-y-auto px-6 py-6 border-r border-n-weak min-w-0">
          <div className="flex flex-col gap-8 max-w-xl">
            {/* Header section with large avatar */}
            <div className="flex items-center gap-5">
              <Avatar
                name={contact.name || "Unknown"}
                src={contact.thumbnail || ""}
                size={72}
                hideOfflineStatus
              />
              <div className="flex flex-col gap-1 min-w-0">
                <h1 className="text-xl font-medium truncate text-n-slate-12">
                  {contact.name || "Unknown contact"}
                </h1>
                <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-n-slate-11">
                  {contact.identifier && (
                    <span className="truncate">ID: {contact.identifier}</span>
                  )}
                  {contact.created_at && (
                    <span>
                      {t("CONTACTS_LAYOUT.DETAILS.CREATED_AT", {
                        date: timeAgo(contact.created_at),
                      })}
                    </span>
                  )}
                  {contact.last_activity_at && (
                    <span>
                      •{" "}
                      {t("CONTACTS_LAYOUT.DETAILS.LAST_ACTIVITY", {
                        date: timeAgo(contact.last_activity_at),
                      })}
                    </span>
                  )}
                </div>
              </div>
            </div>

            {updateFeedback && (
              <div
                className={`rounded-lg p-3 text-sm ${
                  updateFeedback.type === "success"
                    ? "bg-n-teal-9/10 border border-n-teal-8/30 text-n-teal-11"
                    : "bg-n-ruby-9/10 border border-n-ruby-8/30 text-n-ruby-11"
                }`}
              >
                {updateFeedback.message}
              </div>
            )}

            {/* Profile Form */}
            <form onSubmit={handleUpdate} className="flex flex-col gap-6">
              <div className="flex flex-col gap-4">
                <h2 className="text-sm font-medium text-n-slate-12">
                  {t("CONTACTS_LAYOUT.CARD.EDIT_DETAILS_FORM.TITLE")}
                </h2>
                <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
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
                    placeholder="Email"
                    type="email"
                    modelValue={email}
                    onModelValueChange={(v) => setEmail(String(v))}
                  />
                  <Input
                    label="Phone Number"
                    placeholder="Phone"
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
                  placeholder="Biography"
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

              <div>
                <Button
                  type="submit"
                  variant="solid"
                  color="blue"
                  size="sm"
                  label={t("CONTACTS_LAYOUT.CARD.EDIT_DETAILS_FORM.UPDATE_BUTTON")}
                  isLoading={isUpdating}
                  disabled={isUpdating || !firstName.trim()}
                />
              </div>
            </form>

            {/* Delete Contact Section */}
            <div className="flex flex-col gap-3 pt-6 border-t border-n-weak">
              <div className="flex flex-col gap-1">
                <h3 className="text-base font-medium text-n-slate-12">
                  {t("CONTACTS_LAYOUT.DETAILS.DELETE_CONTACT")}
                </h3>
                <p className="text-xs text-n-slate-11">
                  {t("CONTACTS_LAYOUT.DETAILS.DELETE_CONTACT_DESCRIPTION")}
                </p>
              </div>
              <div>
                <Button
                  type="button"
                  variant="outline"
                  color="ruby"
                  size="sm"
                  label={t("CONTACTS_LAYOUT.DETAILS.DELETE_CONTACT")}
                  onClick={() => setDeleteDialogOpen(true)}
                />
              </div>
            </div>
          </div>
        </main>

        {/* Right column: Sidebar with tabs (Notes, Conversations, Merge) */}
        <aside className="w-96 shrink-0 flex flex-col bg-n-solid-2 overflow-y-auto">
          <div className="p-4 border-b border-n-weak">
            <TabBar
              tabs={tabs}
              initialActiveTab={activeTabIndex}
              onTabChanged={(tab) => {
                const k = tab.key as "notes" | "conversations" | "merge";
                setActiveTabKey(k);
                if (k === "merge") {
                  setMergeDialogOpen(true);
                }
              }}
            />
          </div>

          <div className="flex-1 py-4">
            {activeTabKey === "notes" && (
              <ContactNotes
                accountId={accountId}
                contactId={contactId}
                currentUserId={me?.id}
              />
            )}
            {activeTabKey === "conversations" && (
              <ContactConversations accountId={accountId} contactId={contactId} />
            )}
            {activeTabKey === "merge" && (
              <div className="px-6 py-4 flex flex-col gap-4 text-center">
                <p className="text-xs text-n-slate-11">
                  {t("CONTACTS_LAYOUT.SIDEBAR.MERGE.DESCRIPTION")}
                </p>
                <Button
                  variant="solid"
                  color="blue"
                  size="sm"
                  label={t("CONTACTS_LAYOUT.SIDEBAR.MERGE.TITLE")}
                  onClick={() => setMergeDialogOpen(true)}
                />
              </div>
            )}
          </div>
        </aside>
      </div>

      {/* Merge Dialog */}
      <ContactMergeDialog
        accountId={accountId}
        contact={contact}
        open={mergeDialogOpen}
        onClose={() => setMergeDialogOpen(false)}
        onMerged={(baseId) => {
          setMergeDialogOpen(false);
          navigate(`/accounts/${accountId}/contacts/${baseId}`);
        }}
      />

      {/* Delete Dialog */}
      <ConfirmContactDeleteDialog
        open={deleteDialogOpen}
        contact={contact}
        onClose={() => setDeleteDialogOpen(false)}
        onConfirm={handleDelete}
        isLoading={isDeleting}
      />
    </div>
  );
}
