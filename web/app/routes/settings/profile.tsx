import { Bell, Check, Copy, Eye, EyeOff, KeyRound } from "lucide-react";
import { useEffect, useState } from "react";
import { useOutletContext } from "react-router";
import Avatar from "~/components/next/Avatar";
import Button from "~/components/next/Button";
import Switch from "~/components/next/Switch";
import { acct, api } from "~/lib/api";
import { t } from "~/lib/i18n";
import type { ShellCtx } from "../shell";

type UserProfile = {
  id: number;
  name: string;
  display_name: string | null;
  available_name: string;
  email: string;
  avatar_url: string;
  message_signature: string | null;
  access_token?: string;
};

type NotificationSettingsResponse = {
  id: number;
  user_id: number;
  account_id: number;
  selected_email_flags: string[];
  selected_push_flags: string[];
};

const NOTIFICATION_TYPES = [
  {
    label: "PROFILE_SETTINGS.FORM.NOTIFICATIONS.TYPES.CONVERSATION_CREATED",
    value: "conversation_creation",
  },
  {
    label: "PROFILE_SETTINGS.FORM.NOTIFICATIONS.TYPES.CONVERSATION_ASSIGNED",
    value: "conversation_assignment",
  },
  {
    label: "PROFILE_SETTINGS.FORM.NOTIFICATIONS.TYPES.CONVERSATION_MENTION",
    value: "conversation_mention",
  },
  {
    label: "PROFILE_SETTINGS.FORM.NOTIFICATIONS.TYPES.ASSIGNED_CONVERSATION_NEW_MESSAGE",
    value: "assigned_conversation_new_message",
  },
  {
    label: "PROFILE_SETTINGS.FORM.NOTIFICATIONS.TYPES.PARTICIPATING_CONVERSATION_NEW_MESSAGE",
    value: "participating_conversation_new_message",
  },
  {
    label: "PROFILE_SETTINGS.FORM.NOTIFICATIONS.TYPES.SLA_MISSED_FIRST_RESPONSE",
    value: "sla_missed_first_response",
  },
  {
    label: "PROFILE_SETTINGS.FORM.NOTIFICATIONS.TYPES.SLA_MISSED_NEXT_RESPONSE",
    value: "sla_missed_next_response",
  },
  {
    label: "PROFILE_SETTINGS.FORM.NOTIFICATIONS.TYPES.SLA_MISSED_RESOLUTION",
    value: "sla_missed_resolution",
  },
  {
    label: "PROFILE_SETTINGS.FORM.NOTIFICATIONS.TYPES.VOICE_CALL_MISSED",
    value: "voice_call_missed",
  },
];

const inputClass =
  "w-full rounded-lg border border-n-weak bg-n-background px-3 py-2 text-sm text-n-slate-12 outline-none focus:border-n-brand";

export default function ProfileSettings() {
  const { accountId, me } = useOutletContext<ShellCtx>();

  // Profile basic details state
  const [name, setName] = useState(me.name || "");
  const [displayName, setDisplayName] = useState(me.available_name || "");
  const [email, setEmail] = useState(me.email || "");
  const [initialEmail, setInitialEmail] = useState(me.email || "");
  const [avatarUrl, setAvatarUrl] = useState(me.avatar_url || "");
  const [messageSignature, setMessageSignature] = useState("");
  const [accessToken, setAccessToken] = useState("");

  // Change password state
  const [currentPassword, setCurrentPassword] = useState("");
  const [password, setPassword] = useState("");
  const [passwordConfirmation, setPasswordConfirmation] = useState("");

  // Notification settings state
  const [selectedEmailFlags, setSelectedEmailFlags] = useState<string[]>([]);
  const [selectedPushFlags, setSelectedPushFlags] = useState<string[]>([]);
  const [hasPush, setHasPush] = useState(
    typeof window !== "undefined" &&
      typeof Notification !== "undefined" &&
      Notification.permission === "granted"
  );

  // Access token state
  const [showToken, setShowToken] = useState(false);
  const [copiedToken, setCopiedToken] = useState(false);
  const [confirmReset, setConfirmReset] = useState(false);

  // Status & feedback
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [statusMsg, setStatusMsg] = useState("");

  useEffect(() => {
    let active = true;
    async function loadData() {
      try {
        const [profileRes, notifRes] = await Promise.all([
          api<UserProfile>("/api/v1/profile"),
          api<NotificationSettingsResponse>(
            `${acct(accountId)}/notification_settings`
          ).catch(() => null),
        ]);

        if (!active) return;

        if (profileRes) {
          setName(profileRes.name || "");
          setDisplayName(profileRes.display_name || "");
          setEmail(profileRes.email || "");
          setInitialEmail(profileRes.email || "");
          setAvatarUrl(profileRes.avatar_url || "");
          setMessageSignature(profileRes.message_signature || "");
          setAccessToken(profileRes.access_token || "");
        }

        if (notifRes) {
          setSelectedEmailFlags(notifRes.selected_email_flags || []);
          setSelectedPushFlags(notifRes.selected_push_flags || []);
        }
      } catch (e) {
        if (active) {
          setError(e instanceof Error ? e.message : "Failed to load profile");
        }
      } finally {
        if (active) setLoading(false);
      }
    }

    loadData();
    return () => {
      active = false;
    };
  }, [accountId]);

  // Handlers
  async function handleAvatarUpload(file: File) {
    setBusy(true);
    setError("");
    setStatusMsg("");
    try {
      const formData = new FormData();
      formData.append("profile[avatar]", file);
      const res = await api<UserProfile>("/api/v1/profile", {
        method: "PUT",
        body: formData,
      });
      setAvatarUrl(res.avatar_url || "");
      setStatusMsg(t("PROFILE_SETTINGS.UPDATE_SUCCESS"));
    } catch (e) {
      setError(
        e instanceof Error
          ? e.message
          : t("PROFILE_SETTINGS.AVATAR_DELETE_FAILED")
      );
    } finally {
      setBusy(false);
    }
  }

  async function handleAvatarDelete() {
    setBusy(true);
    setError("");
    setStatusMsg("");
    try {
      const res = await api<UserProfile>("/api/v1/profile/avatar", {
        method: "DELETE",
      });
      setAvatarUrl(res.avatar_url || "");
      setStatusMsg(t("PROFILE_SETTINGS.AVATAR_DELETE_SUCCESS"));
    } catch (e) {
      setError(
        e instanceof Error
          ? e.message
          : t("PROFILE_SETTINGS.AVATAR_DELETE_FAILED")
      );
    } finally {
      setBusy(false);
    }
  }

  async function handleUpdateProfile(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError("");
    setStatusMsg("");
    const hasEmailChanged = initialEmail !== email;
    try {
      const res = await api<UserProfile>("/api/v1/profile", {
        method: "PUT",
        json: {
          profile: {
            name,
            display_name: displayName,
            email,
          },
        },
      });
      setName(res.name || "");
      setDisplayName(res.display_name || "");
      setEmail(res.email || "");
      setInitialEmail(res.email || "");
      setStatusMsg(
        hasEmailChanged
          ? t("PROFILE_SETTINGS.AFTER_EMAIL_CHANGED")
          : t("PROFILE_SETTINGS.UPDATE_SUCCESS")
      );
    } catch (e) {
      setError(
        e instanceof Error ? e.message : t("RESET_PASSWORD.API.ERROR_MESSAGE")
      );
    } finally {
      setBusy(false);
    }
  }

  async function handleUpdateSignature(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError("");
    setStatusMsg("");
    try {
      const res = await api<UserProfile>("/api/v1/profile", {
        method: "PUT",
        json: {
          profile: {
            message_signature: messageSignature,
          },
        },
      });
      setMessageSignature(res.message_signature || "");
      setStatusMsg(
        t("PROFILE_SETTINGS.FORM.MESSAGE_SIGNATURE_SECTION.API_SUCCESS")
      );
    } catch (e) {
      setError(
        e instanceof Error
          ? e.message
          : t("PROFILE_SETTINGS.FORM.MESSAGE_SIGNATURE_SECTION.API_ERROR")
      );
    } finally {
      setBusy(false);
    }
  }

  async function handleChangePassword(e: React.FormEvent) {
    e.preventDefault();
    if (!currentPassword) {
      setError(t("PROFILE_SETTINGS.FORM.CURRENT_PASSWORD.ERROR"));
      return;
    }
    if (password.length < 6) {
      setError(t("PROFILE_SETTINGS.FORM.PASSWORD.ERROR"));
      return;
    }
    if (password !== passwordConfirmation) {
      setError(t("PROFILE_SETTINGS.FORM.PASSWORD_CONFIRMATION.ERROR"));
      return;
    }

    setBusy(true);
    setError("");
    setStatusMsg("");
    try {
      await api("/api/v1/profile", {
        method: "PUT",
        json: {
          profile: {
            current_password: currentPassword,
            password,
            password_confirmation: passwordConfirmation,
          },
        },
      });
      setCurrentPassword("");
      setPassword("");
      setPasswordConfirmation("");
      setStatusMsg(t("PROFILE_SETTINGS.PASSWORD_UPDATE_SUCCESS"));
    } catch (e) {
      setError(
        e instanceof Error ? e.message : t("RESET_PASSWORD.API.ERROR_MESSAGE")
      );
    } finally {
      setBusy(false);
    }
  }

  async function handleToggleFlag(type: "email" | "push", flagName: string) {
    const flagKey = `${type}_${flagName}`;
    const isEmail = type === "email";
    const currentList = isEmail ? selectedEmailFlags : selectedPushFlags;
    const nextList = currentList.includes(flagKey)
      ? currentList.filter((f) => f !== flagKey)
      : [...currentList, flagKey];

    if (isEmail) {
      setSelectedEmailFlags(nextList);
    } else {
      setSelectedPushFlags(nextList);
    }

    try {
      await api(`${acct(accountId)}/notification_settings`, {
        method: "PUT",
        json: {
          notification_settings: {
            selected_email_flags: isEmail ? nextList : selectedEmailFlags,
            selected_push_flags: isEmail ? selectedPushFlags : nextList,
          },
        },
      });
      setStatusMsg(t("PROFILE_SETTINGS.FORM.API.UPDATE_SUCCESS"));
    } catch (e) {
      setError(
        e instanceof Error
          ? e.message
          : t("PROFILE_SETTINGS.FORM.API.UPDATE_ERROR")
      );
    }
  }

  async function handleTogglePush(enabled: boolean) {
    if (enabled && typeof Notification !== "undefined") {
      const perm = await Notification.requestPermission();
      setHasPush(perm === "granted");
    } else {
      setHasPush(false);
    }
  }

  async function handleCopyToken() {
    if (!accessToken) return;
    try {
      await navigator.clipboard.writeText(accessToken);
      setCopiedToken(true);
      setTimeout(() => setCopiedToken(false), 2000);
      setStatusMsg(t("COMPONENTS.CODE.COPY_SUCCESSFUL"));
    } catch {
      setError(t("COMPONENTS.CODE.COPY_ERROR"));
    }
  }

  async function handleResetToken() {
    if (!confirmReset) {
      setConfirmReset(true);
      return;
    }
    setBusy(true);
    setError("");
    setStatusMsg("");
    try {
      const res = await api<UserProfile>("/api/v1/profile/reset_access_token", {
        method: "POST",
      });
      setAccessToken(res.access_token || "");
      setConfirmReset(false);
      setStatusMsg(t("PROFILE_SETTINGS.FORM.ACCESS_TOKEN.RESET_SUCCESS"));
    } catch (e) {
      setError(
        e instanceof Error
          ? e.message
          : t("PROFILE_SETTINGS.FORM.ACCESS_TOKEN.RESET_ERROR")
      );
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="flex-1 overflow-auto p-8 text-n-slate-12">
      <div className="max-w-2xl space-y-8">
        <header>
          <h1 className="text-xl font-medium">{t("PROFILE_SETTINGS.TITLE")}</h1>
        </header>

        {error && (
          <p
            role="alert"
            className="rounded-lg border border-n-ruby-9/20 bg-n-ruby-9/10 p-3 text-sm text-n-ruby-11"
          >
            {error}
          </p>
        )}
        {statusMsg && (
          <p
            role="status"
            className="rounded-lg border border-n-teal-9/20 bg-n-teal-9/10 p-3 text-sm text-n-teal-11"
          >
            {statusMsg}
          </p>
        )}

        {loading ? (
          <p className="text-sm text-n-slate-11">
            {t("LABEL_MGMT.LOADING") || "Loading..."}
          </p>
        ) : (
          <>
            {/* Section 1: Profile & Avatar */}
            <div className="space-y-6 border-b border-n-weak pb-8">
              <div className="flex flex-col gap-2">
                <span className="text-sm font-medium text-n-slate-12">
                  {t("PROFILE_SETTINGS.FORM.PICTURE")}
                </span>
                <Avatar
                  src={avatarUrl}
                  name={displayName || name || me.available_name}
                  size={72}
                  allowUpload
                  onUpload={({ file }) => handleAvatarUpload(file)}
                  onDelete={handleAvatarDelete}
                />
              </div>

              <form onSubmit={handleUpdateProfile} className="grid gap-4">
                <label className="grid gap-1 text-sm">
                  {t("PROFILE_SETTINGS.FORM.NAME.LABEL")}
                  <input
                    required
                    value={name}
                    onChange={(e) => setName(e.target.value)}
                    placeholder={t("PROFILE_SETTINGS.FORM.NAME.PLACEHOLDER")}
                    className={inputClass}
                  />
                </label>

                <label className="grid gap-1 text-sm">
                  {t("PROFILE_SETTINGS.FORM.DISPLAY_NAME.LABEL")}
                  <input
                    value={displayName}
                    onChange={(e) => setDisplayName(e.target.value)}
                    placeholder={t(
                      "PROFILE_SETTINGS.FORM.DISPLAY_NAME.PLACEHOLDER"
                    )}
                    className={inputClass}
                  />
                </label>

                <label className="grid gap-1 text-sm">
                  {t("PROFILE_SETTINGS.FORM.EMAIL.LABEL")}
                  <input
                    required
                    type="email"
                    value={email}
                    onChange={(e) => setEmail(e.target.value)}
                    placeholder={t("PROFILE_SETTINGS.FORM.EMAIL.PLACEHOLDER")}
                    className={inputClass}
                  />
                </label>

                <div>
                  <Button
                    type="submit"
                    disabled={busy}
                    label={t("PROFILE_SETTINGS.BTN_TEXT")}
                  />
                </div>
              </form>
            </div>

            {/* Section 2: Message Signature */}
            <div className="space-y-4 border-b border-n-weak pb-8">
              <div>
                <h2 className="text-lg font-medium">
                  {t("PROFILE_SETTINGS.FORM.MESSAGE_SIGNATURE_SECTION.TITLE")}
                </h2>
                <p className="mt-1 text-sm text-n-slate-11">
                  {t("PROFILE_SETTINGS.FORM.MESSAGE_SIGNATURE_SECTION.NOTE")}
                </p>
              </div>

              <form onSubmit={handleUpdateSignature} className="grid gap-4">
                <label className="grid gap-1 text-sm">
                  <textarea
                    rows={4}
                    value={messageSignature}
                    onChange={(e) => setMessageSignature(e.target.value)}
                    placeholder={t(
                      "PROFILE_SETTINGS.FORM.MESSAGE_SIGNATURE.PLACEHOLDER"
                    )}
                    className={inputClass}
                  />
                </label>

                <div>
                  <Button
                    type="submit"
                    disabled={busy}
                    label={t(
                      "PROFILE_SETTINGS.FORM.MESSAGE_SIGNATURE_SECTION.BTN_TEXT"
                    )}
                  />
                </div>
              </form>
            </div>

            {/* Section 3: Change Password */}
            <div className="space-y-4 border-b border-n-weak pb-8">
              <div>
                <h2 className="text-lg font-medium">
                  {t("PROFILE_SETTINGS.FORM.PASSWORD_SECTION.TITLE")}
                </h2>
                <p className="mt-1 text-sm text-n-slate-11">
                  {t("PROFILE_SETTINGS.FORM.PASSWORD_SECTION.NOTE")}
                </p>
              </div>

              <form onSubmit={handleChangePassword} className="grid gap-4">
                <label className="grid gap-1 text-sm">
                  {t("PROFILE_SETTINGS.FORM.CURRENT_PASSWORD.LABEL")}
                  <input
                    type="password"
                    required
                    value={currentPassword}
                    onChange={(e) => setCurrentPassword(e.target.value)}
                    placeholder={t(
                      "PROFILE_SETTINGS.FORM.CURRENT_PASSWORD.PLACEHOLDER"
                    )}
                    className={inputClass}
                  />
                </label>

                <label className="grid gap-1 text-sm">
                  {t("PROFILE_SETTINGS.FORM.PASSWORD.LABEL")}
                  <input
                    type="password"
                    required
                    minLength={6}
                    value={password}
                    onChange={(e) => setPassword(e.target.value)}
                    placeholder={t("PROFILE_SETTINGS.FORM.PASSWORD.PLACEHOLDER")}
                    className={inputClass}
                  />
                </label>

                <label className="grid gap-1 text-sm">
                  {t("PROFILE_SETTINGS.FORM.PASSWORD_CONFIRMATION.LABEL")}
                  <input
                    type="password"
                    required
                    minLength={6}
                    value={passwordConfirmation}
                    onChange={(e) => setPasswordConfirmation(e.target.value)}
                    placeholder={t(
                      "PROFILE_SETTINGS.FORM.PASSWORD_CONFIRMATION.PLACEHOLDER"
                    )}
                    className={inputClass}
                  />
                </label>

                <div>
                  <Button
                    type="submit"
                    disabled={
                      busy ||
                      !currentPassword ||
                      !password ||
                      password !== passwordConfirmation
                    }
                    label={t(
                      "PROFILE_SETTINGS.FORM.PASSWORD_SECTION.BTN_TEXT"
                    )}
                  />
                </div>
              </form>
            </div>

            {/* Section 4: Notification Preferences */}
            <div className="space-y-4 border-b border-n-weak pb-8">
              <div>
                <h2 className="text-lg font-medium">
                  {t("PROFILE_SETTINGS.FORM.NOTIFICATIONS.TITLE")}
                </h2>
              </div>

              <div className="overflow-x-auto">
                <table className="w-full text-left text-sm">
                  <thead>
                    <tr className="border-b border-n-weak text-n-slate-11">
                      <th className="py-3 pr-4 font-medium">
                        {t(
                          "PROFILE_SETTINGS.FORM.NOTIFICATIONS.TYPE_TITLE"
                        )}
                      </th>
                      <th className="px-4 py-3 text-center font-medium">
                        {t("PROFILE_SETTINGS.FORM.NOTIFICATIONS.EMAIL")}
                      </th>
                      <th className="px-4 py-3 text-center font-medium">
                        {t("PROFILE_SETTINGS.FORM.NOTIFICATIONS.PUSH")}
                      </th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-n-weak">
                    {NOTIFICATION_TYPES.map((notification) => {
                      const emailChecked = selectedEmailFlags.includes(
                        `email_${notification.value}`
                      );
                      const pushChecked = selectedPushFlags.includes(
                        `push_${notification.value}`
                      );
                      return (
                        <tr key={notification.value}>
                          <td className="py-3 pr-4 text-n-slate-12">
                            {t(notification.label)}
                          </td>
                          <td className="px-4 py-3 text-center">
                            <input
                              type="checkbox"
                              checked={emailChecked}
                              onChange={() =>
                                handleToggleFlag("email", notification.value)
                              }
                              className="h-4 w-4 rounded border-n-weak text-n-brand focus:ring-n-brand"
                            />
                          </td>
                          <td className="px-4 py-3 text-center">
                            <input
                              type="checkbox"
                              checked={pushChecked}
                              onChange={() =>
                                handleToggleFlag("push", notification.value)
                              }
                              className="h-4 w-4 rounded border-n-weak text-n-brand focus:ring-n-brand"
                            />
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>

              <div className="flex items-center justify-between rounded-xl border border-n-weak p-4 text-sm">
                <div className="flex items-center gap-2">
                  <Bell className="size-4 shrink-0 text-n-slate-12" />
                  <span>
                    {t(
                      "PROFILE_SETTINGS.FORM.NOTIFICATIONS.BROWSER_PERMISSION"
                    )}
                  </span>
                </div>
                <Switch
                  modelValue={hasPush}
                  onModelValueChange={handleTogglePush}
                />
              </div>
            </div>

            {/* Section 5: Access Token */}
            <div className="space-y-4 pb-8">
              <div>
                <h2 className="text-lg font-medium">
                  {t("PROFILE_SETTINGS.FORM.ACCESS_TOKEN.TITLE")}
                </h2>
                <p className="mt-1 text-sm text-n-slate-11">
                  {t("PROFILE_SETTINGS.FORM.ACCESS_TOKEN.NOTE")}
                </p>
              </div>

              <div className="flex flex-col gap-3 sm:flex-row sm:items-center">
                <div className="relative flex-1">
                  <input
                    type={showToken ? "text" : "password"}
                    readOnly
                    value={accessToken}
                    className={`${inputClass} pr-10 font-mono`}
                  />
                  <button
                    type="button"
                    onClick={() => setShowToken(!showToken)}
                    className="absolute right-3 top-1/2 -translate-y-1/2 text-n-slate-10 hover:text-n-slate-12"
                    aria-label={showToken ? "Hide token" : "Show token"}
                  >
                    {showToken ? (
                      <EyeOff className="size-4" />
                    ) : (
                      <Eye className="size-4" />
                    )}
                  </button>
                </div>

                <div className="flex items-center gap-2">
                  <Button
                    type="button"
                    slate
                    outline
                    label={t("PROFILE_SETTINGS.FORM.ACCESS_TOKEN.COPY")}
                    icon={copiedToken ? Check : Copy}
                    onClick={handleCopyToken}
                  />

                  {confirmReset ? (
                    <div className="flex items-center gap-1">
                      <Button
                        type="button"
                        ruby
                        disabled={busy}
                        label={t(
                          "PROFILE_SETTINGS.FORM.ACCESS_TOKEN.CONFIRM_RESET"
                        )}
                        onClick={handleResetToken}
                      />
                      <Button
                        type="button"
                        slate
                        ghost
                        label={t("LABEL_MGMT.FORM.CANCEL") || "Cancel"}
                        onClick={() => setConfirmReset(false)}
                      />
                    </div>
                  ) : (
                    <Button
                      type="button"
                      slate
                      outline
                      label={t("PROFILE_SETTINGS.FORM.ACCESS_TOKEN.RESET")}
                      icon={KeyRound}
                      onClick={() => setConfirmReset(true)}
                    />
                  )}
                </div>
              </div>
            </div>
          </>
        )}
      </div>
    </section>
  );
}
