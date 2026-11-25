import { useEffect, useState } from "react";
import { useOutletContext } from "react-router";
import { Button, ErrorText, Input, PageHeader } from "~/components/ui";
import { acct, api } from "~/lib/api";
import { t } from "~/lib/i18n";
import type { ShellCtx } from "../../shell";

type SamlSettings = {
  id: number | null;
  sso_url: string | null;
  certificate: string | null;
  fingerprint?: string | null;
  idp_entity_id: string | null;
  sp_entity_id: string | null;
  metadata_url: string | null;
};

type SessionRow = {
  id: number;
  browser_name: string;
  ip_address?: string | null;
  last_activity_at: number;
  current: boolean;
};

export function SecuritySettings() {
  const { accountId } = useOutletContext<ShellCtx>();
  const [enforce, setEnforce] = useState(false);
  const [saml, setSaml] = useState<SamlSettings | null>(null);
  const [ssoUrl, setSsoUrl] = useState("");
  const [idpEntityId, setIdpEntityId] = useState("");
  const [metadataUrl, setMetadataUrl] = useState("");
  const [certificate, setCertificate] = useState("");
  const [sessions, setSessions] = useState<SessionRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [status, setStatus] = useState("");

  async function loadAll() {
    const [acc, samlRes, sessRes] = await Promise.all([
      api<{ settings?: { enforce_mfa?: boolean } }>(acct(accountId)),
      api<SamlSettings>(`${acct(accountId)}/saml_settings`).catch(() => null),
      api<SessionRow[]>("/api/v1/profile/sessions").catch(() => []),
    ]);
    setEnforce(acc.settings?.enforce_mfa === true);
    setSaml(samlRes);
    if (samlRes) {
      setSsoUrl(samlRes.sso_url ?? "");
      setIdpEntityId(samlRes.idp_entity_id ?? "");
      setMetadataUrl(samlRes.metadata_url ?? "");
      setCertificate(samlRes.certificate ?? "");
    }
    setSessions(sessRes);
  }

  useEffect(() => {
    let alive = true;
    setLoading(true);
    loadAll()
      .catch((e) => {
        if (alive) setError(e instanceof Error ? e.message : "Request failed");
      })
      .finally(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [accountId]);

  async function saveSaml(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError("");
    setStatus("");
    try {
      const saved = await api<SamlSettings>(`${acct(accountId)}/saml_settings`, {
        method: saml?.id ? "PATCH" : "POST",
        json: {
          saml_settings: {
            sso_url: ssoUrl.trim() || null,
            idp_entity_id: idpEntityId.trim() || null,
            metadata_url: metadataUrl.trim() || null,
            certificate: certificate.trim() || null,
          },
        },
      });
      setSaml(saved);
      setStatus("SAML settings saved.");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to save SAML settings");
    } finally {
      setBusy(false);
    }
  }

  async function refreshMetadata() {
    setBusy(true);
    setError("");
    setStatus("");
    try {
      const updated = await api<SamlSettings>(`${acct(accountId)}/saml_settings/refresh`, { method: "POST" });
      setSaml(updated);
      setSsoUrl(updated.sso_url ?? "");
      setIdpEntityId(updated.idp_entity_id ?? "");
      setCertificate(updated.certificate ?? "");
      setStatus("SAML metadata refreshed.");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to refresh metadata");
    } finally {
      setBusy(false);
    }
  }

  async function removeSaml() {
    if (!window.confirm("Disable and delete SAML settings?")) return;
    setBusy(true);
    setError("");
    try {
      await api(`${acct(accountId)}/saml_settings`, { method: "DELETE" });
      await loadAll();
      setStatus("SAML settings removed.");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to remove SAML settings");
    } finally {
      setBusy(false);
    }
  }

  async function revokeSession(sessionId: number) {
    setBusy(true);
    setError("");
    try {
      await api(`/api/v1/profile/sessions/${sessionId}`, { method: "DELETE" });
      setSessions(await api<SessionRow[]>("/api/v1/profile/sessions"));
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to revoke session");
    } finally {
      setBusy(false);
    }
  }

  async function clearTrustedDevices() {
    setBusy(true);
    setError("");
    try {
      await api("/api/v1/profile/trusted_devices", { method: "DELETE" });
      setStatus("Trusted devices cleared.");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to clear trusted devices");
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="flex min-w-0 flex-1 flex-col">
      <PageHeader title={t("SECURITY_SETTINGS.TITLE")} />
      <div className="flex-1 space-y-6 overflow-auto p-6">
        <p className="max-w-2xl text-sm text-ink-3">{t("SECURITY_SETTINGS.DESCRIPTION")}</p>
        <ErrorText>{error}</ErrorText>
        {status && <p role="status" className="text-sm text-ink-2">{status}</p>}

        {!loading && (
          <>
            <div className="flex max-w-2xl items-center justify-between gap-4 rounded-lg border border-line p-4">
              <div>
                <p className="text-sm font-medium">{t("MFA_SETTINGS.TITLE")}</p>
                <p className="mt-1 text-sm text-ink-3">{t("MFA_SETTINGS.DESCRIPTION")}</p>
              </div>
              <span className="text-sm text-ink-2">
                {enforce ? t("MFA_SETTINGS.ENABLED") : t("MFA_SETTINGS.DISABLED")}
              </span>
            </div>

            <form onSubmit={saveSaml} className="max-w-2xl space-y-4 rounded-lg border border-line p-5">
              <div className="flex items-center justify-between">
                <h2 className="text-sm font-medium">SAML Single Sign-On</h2>
                {saml?.sp_entity_id && (
                  <span className="text-xs text-ink-3">SP Entity ID: {saml.sp_entity_id}</span>
                )}
              </div>
              <Input label="SSO URL" type="url" value={ssoUrl} onChange={(e) => setSsoUrl(e.target.value)} />
              <Input label="IdP Entity ID" value={idpEntityId} onChange={(e) => setIdpEntityId(e.target.value)} />
              <Input label="Metadata URL" type="url" value={metadataUrl} onChange={(e) => setMetadataUrl(e.target.value)} />
              <label className="grid gap-1.5 text-[13px] text-ink-2">
                X.509 Certificate
                <textarea
                  rows={4}
                  value={certificate}
                  onChange={(e) => setCertificate(e.target.value)}
                  className="w-full rounded-lg border border-line-strong bg-surface p-3 font-mono text-xs text-ink outline-none focus:border-accent"
                />
              </label>
              <div className="flex flex-wrap gap-2">
                <Button type="submit" disabled={busy}>
                  Save SAML
                </Button>
                {saml?.id && metadataUrl && (
                  <Button type="button" variant="quiet" disabled={busy} onClick={refreshMetadata}>
                    Refresh metadata
                  </Button>
                )}
                {saml?.id && (
                  <Button type="button" variant="danger" disabled={busy} onClick={removeSaml}>
                    Delete SAML
                  </Button>
                )}
              </div>
            </form>

            <div className="max-w-2xl space-y-3 rounded-lg border border-line p-5">
              <div className="flex items-center justify-between">
                <h2 className="text-sm font-medium">Active Sessions</h2>
                <Button variant="quiet" disabled={busy} onClick={clearTrustedDevices}>
                  Clear trusted devices
                </Button>
              </div>
              {sessions.length === 0 ? (
                <p className="text-sm text-ink-3">No active sessions found.</p>
              ) : (
                <ul className="divide-y divide-line">
                  {sessions.map((s) => (
                    <li key={s.id} className="flex items-center justify-between py-2.5 text-sm">
                      <div>
                        <p className="font-medium text-ink">
                          {s.browser_name} {s.current && <span className="text-xs text-accent">(Current)</span>}
                        </p>
                        <p className="text-xs text-ink-3">
                          {s.ip_address || "Local"} · Active{" "}
                          {new Date(s.last_activity_at * 1000).toLocaleString()}
                        </p>
                      </div>
                      {!s.current && (
                        <Button variant="danger" disabled={busy} onClick={() => revokeSession(s.id)}>
                          Revoke
                        </Button>
                      )}
                    </li>
                  ))}
                </ul>
              )}
            </div>
          </>
        )}
      </div>
    </section>
  );
}

// mirrors chatwoot/app/javascript/dashboard/routes/dashboard/conversationWorkflow/index.vue
export function ConversationWorkflow() {
  const { accountId } = useOutletContext<ShellCtx>();
  const [duration, setDuration] = useState("");
  const [message, setMessage] = useState("");
  const [ignoreWaiting, setIgnoreWaiting] = useState(false);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    let alive = true;
    api<{ settings?: Record<string, unknown> }>(acct(accountId))
      .then((a) => {
        if (!alive) return;
        const s = a.settings ?? {};
        setDuration(s.auto_resolve_after == null ? "" : String(s.auto_resolve_after));
        setMessage(String(s.auto_resolve_message ?? ""));
        setIgnoreWaiting(s.auto_resolve_ignore_waiting === true);
      })
      .catch((e) => {
        if (alive) setError(e instanceof Error ? e.message : "Request failed");
      })
      .finally(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [accountId]);

  async function save(e: React.FormEvent) {
    e.preventDefault();
    const minutes = Number(duration);
    if (duration !== "" && (!Number.isInteger(minutes) || minutes < 10 || minutes > 999 * 24 * 60)) {
      setError(t("GENERAL_SETTINGS.FORM.AUTO_RESOLVE.DURATION.ERROR", { minDuration: 10 }));
      return;
    }
    setSaving(true);
    setError("");
    setSaved(false);
    try {
      await api(acct(accountId), {
        method: "PATCH",
        json: {
          settings: {
            auto_resolve_after: duration === "" ? null : minutes,
            auto_resolve_message: message,
            auto_resolve_ignore_waiting: ignoreWaiting,
          },
        },
      });
      setSaved(true);
    } catch (err) {
      setError(err instanceof Error ? err.message : t("GENERAL_SETTINGS.FORM.AUTO_RESOLVE.DURATION.API.ERROR"));
    } finally {
      setSaving(false);
    }
  }

  const input = "w-full rounded-lg border border-n-weak bg-n-background px-3 py-2 text-sm";
  return (
    <section className="flex min-w-0 flex-1 flex-col">
      <PageHeader title={t("CONVERSATION_WORKFLOW.INDEX.HEADER.TITLE")} />
      <div className="flex-1 overflow-auto p-6">
        <p className="mb-6 max-w-2xl text-sm text-ink-3">{t("CONVERSATION_WORKFLOW.INDEX.HEADER.DESCRIPTION")}</p>
        {loading ? null : (
          <form onSubmit={save} className="grid max-w-2xl gap-5 rounded-lg border border-line p-5">
            <h2 className="text-sm font-medium">{t("GENERAL_SETTINGS.FORM.AUTO_RESOLVE.TITLE")}</h2>
            <p className="text-sm text-ink-3">{t("GENERAL_SETTINGS.FORM.AUTO_RESOLVE.NOTE")}</p>
            <label className="grid gap-1 text-sm">
              {t("GENERAL_SETTINGS.FORM.AUTO_RESOLVE.DURATION.LABEL")}
              <input
                className={input}
                type="number"
                min="0"
                value={duration}
                placeholder={t("GENERAL_SETTINGS.FORM.AUTO_RESOLVE.DURATION.PLACEHOLDER")}
                onChange={(e) => setDuration(e.target.value)}
              />
              <span className="text-ink-3">{t("GENERAL_SETTINGS.FORM.AUTO_RESOLVE.DURATION.HELP")}</span>
            </label>
            <label className="grid gap-1 text-sm">
              {t("GENERAL_SETTINGS.FORM.AUTO_RESOLVE.MESSAGE.LABEL")}
              <textarea
                className={input}
                rows={3}
                value={message}
                placeholder={t("GENERAL_SETTINGS.FORM.AUTO_RESOLVE.MESSAGE.PLACEHOLDER")}
                onChange={(e) => setMessage(e.target.value)}
              />
              <span className="text-ink-3">{t("GENERAL_SETTINGS.FORM.AUTO_RESOLVE.MESSAGE.HELP")}</span>
            </label>
            <label className="flex items-center gap-2 text-sm">
              <input type="checkbox" checked={ignoreWaiting} onChange={(e) => setIgnoreWaiting(e.target.checked)} />
              {t("GENERAL_SETTINGS.FORM.AUTO_RESOLVE.IGNORE_WAITING.LABEL")}
            </label>
            <ErrorText>{error}</ErrorText>
            {saved && <p className="text-sm text-ink-2">{t("GENERAL_SETTINGS.FORM.AUTO_RESOLVE.DURATION.API.SUCCESS")}</p>}
            <div>
              <button type="submit" disabled={saving} className="rounded-lg bg-n-brand px-4 py-2 text-sm text-white disabled:opacity-50">
                {t("GENERAL_SETTINGS.FORM.AUTO_RESOLVE.UPDATE_BUTTON")}
              </button>
            </div>
          </form>
        )}
      </div>
    </section>
  );
}

type MfaStatus = {
  enabled: boolean;
  provisioning_uri?: string;
  backup_codes?: string[];
};

export function ProfileMfa() {
  const [mfa, setMfa] = useState<MfaStatus | null>(null);
  const [code, setCode] = useState("");
  const [error, setError] = useState("");
  const [status, setStatus] = useState("");
  const [busy, setBusy] = useState(false);

  async function load() {
    const res = await api<MfaStatus>("/api/v1/profile/mfa");
    setMfa(res);
  }

  useEffect(() => {
    let alive = true;
    load().catch((e) => {
      if (alive) setError(e instanceof Error ? e.message : "MFA unavailable");
    });
    return () => {
      alive = false;
    };
  }, []);

  async function beginSetup() {
    setBusy(true);
    setError("");
    try {
      const res = await api<MfaStatus>("/api/v1/profile/mfa", { method: "POST" });
      setMfa(res);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to initialize MFA");
    } finally {
      setBusy(false);
    }
  }

  async function verifyCode(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError("");
    try {
      const res = await api<MfaStatus>("/api/v1/profile/mfa/verify", {
        method: "POST",
        json: { otp_code: code.trim() },
      });
      setMfa(res);
      setCode("");
      setStatus("MFA enabled.");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Verification failed");
    } finally {
      setBusy(false);
    }
  }

  async function disableMfa(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError("");
    try {
      const res = await api<MfaStatus>("/api/v1/profile/mfa", {
        method: "DELETE",
        json: { otp_code: code.trim() },
      });
      setMfa(res);
      setCode("");
      setStatus("MFA disabled.");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Could not disable MFA");
    } finally {
      setBusy(false);
    }
  }

  async function regenBackupCodes() {
    if (!code.trim()) {
      setError("Enter your current authenticator code to regenerate backup codes.");
      return;
    }
    setBusy(true);
    setError("");
    try {
      const res = await api<{ backup_codes: string[] }>("/api/v1/profile/mfa/backup_codes", {
        method: "POST",
        json: { otp_code: code.trim() },
      });
      setMfa((prev) => (prev ? { ...prev, backup_codes: res.backup_codes } : prev));
      setCode("");
      setStatus("Backup codes regenerated.");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to regenerate backup codes");
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="flex min-w-0 flex-1 flex-col">
      <PageHeader title={t("MFA_SETTINGS.TITLE")} />
      <div className="flex-1 space-y-5 overflow-auto p-6">
        <p className="max-w-2xl text-sm text-ink-3">{t("MFA_SETTINGS.SUBTITLE")}</p>
        <ErrorText>{error}</ErrorText>
        {status && <p role="status" className="text-sm text-ink-2">{status}</p>}
        {mfa && (
          <div className="max-w-xl space-y-4 rounded-lg border border-line p-5">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-sm font-medium">{t("MFA_SETTINGS.STATUS_TITLE")}</p>
                <p className="mt-1 text-sm text-ink-3">{t("MFA_SETTINGS.STATUS_DESCRIPTION")}</p>
              </div>
              <span className="text-sm font-medium text-ink">
                {mfa.enabled ? t("MFA_SETTINGS.ENABLED") : t("MFA_SETTINGS.DISABLED")}
              </span>
            </div>

            {!mfa.enabled && !mfa.provisioning_uri && (
              <Button disabled={busy} onClick={beginSetup}>
                Enable Two-Factor Auth
              </Button>
            )}

            {mfa.provisioning_uri && (
              <form onSubmit={verifyCode} className="space-y-3 border-t border-line pt-4">
                <p className="text-xs text-ink-2">Add this URI to your authenticator app:</p>
                <code className="block break-all rounded bg-raised p-2 text-xs">{mfa.provisioning_uri}</code>
                <Input label="6-digit OTP code" value={code} onChange={(e) => setCode(e.target.value)} required />
                <Button type="submit" disabled={busy}>
                  Verify & Activate
                </Button>
              </form>
            )}

            {mfa.enabled && (
              <form onSubmit={disableMfa} className="space-y-3 border-t border-line pt-4">
                <Input label="6-digit OTP code" value={code} onChange={(e) => setCode(e.target.value)} required />
                <div className="flex flex-wrap gap-2">
                  <Button type="button" variant="quiet" disabled={busy} onClick={regenBackupCodes}>
                    Regenerate backup codes
                  </Button>
                  <Button type="submit" variant="danger" disabled={busy}>
                    Disable MFA
                  </Button>
                </div>
              </form>
            )}

            {mfa.backup_codes && mfa.backup_codes.length > 0 && (
              <div className="border-t border-line pt-3">
                <p className="mb-2 text-xs font-medium text-ink-2">Backup Codes</p>
                <div className="grid grid-cols-2 gap-1.5 font-mono text-xs">
                  {mfa.backup_codes.map((c) => (
                    <span key={c} className="rounded bg-raised px-2 py-1">
                      {c}
                    </span>
                  ))}
                </div>
              </div>
            )}
          </div>
        )}
      </div>
    </section>
  );
}
