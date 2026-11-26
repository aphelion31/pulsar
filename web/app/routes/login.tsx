import { LockKeyhole, ShieldCheck } from "lucide-react";
import { useEffect, useState } from "react";
import { Link, useNavigate, useSearchParams } from "react-router";
import { AuthShell } from "~/components/next/AuthShell";
import { authResponse } from "~/lib/api";
import { t } from "~/lib/i18n";

const inputCls =
  "block h-10 w-full rounded-lg border-0 bg-n-alpha-black2 px-3 py-2 text-sm text-n-slate-12 outline outline-1 outline-n-weak transition-colors placeholder:text-n-slate-10 hover:outline-n-slate-6 focus:outline-n-brand";

export default function Login() {
  const nav = useNavigate();
  const [searchParams] = useSearchParams();
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [wiggle, setWiggle] = useState(false);
  const [mfaToken, setMfaToken] = useState<string | null>(null);
  const [otpCode, setOtpCode] = useState("");
  const [useBackup, setUseBackup] = useState(false);
  const [googleEnabled, setGoogleEnabled] = useState(false);

  useEffect(() => {
    let active = true;
    fetch("/api")
      .then((r) => (r.ok ? r.json() : null))
      .then((info) => {
        if (active && info?.google_oauth_enabled) setGoogleEnabled(true);
      })
      .catch(() => {});
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    const ssoToken = searchParams.get("sso_auth_token");
    const oauthErr = searchParams.get("error");
    if (oauthErr) {
      setError(oauthErr.replaceAll("_", " "));
    }
    if (!ssoToken) return;
    let active = true;
    setBusy(true);
    fetch("/auth/sso_exchange", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        sso_auth_token: ssoToken,
        email: searchParams.get("email") ?? undefined,
      }),
    })
      .then((res) => authResponse(res))
      .then((me) => {
        if (active) nav(`/accounts/${me.account_id}/conversations`, { replace: true });
      })
      .catch((err) => {
        if (active) setError(err instanceof Error ? err.message : "SSO login failed");
      })
      .finally(() => {
        if (active) setBusy(false);
      });
    return () => {
      active = false;
    };
  }, [searchParams, nav]);

  async function submit(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const f = new FormData(e.currentTarget);
    setBusy(true);
    setError("");
    setWiggle(false);
    try {
      const res = await fetch("/auth/sign_in", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          email: String(f.get("email") ?? ""),
          password: String(f.get("password") ?? ""),
        }),
      });
      if (res.status === 206) {
        const body = await res.json().catch(() => ({}));
        if (body.mfa_required && body.mfa_token) {
          setMfaToken(body.mfa_token);
          return;
        }
      }
      const me = await authResponse(res);
      nav(`/accounts/${me.account_id}/conversations`, { replace: true });
    } catch (err) {
      setError(err instanceof Error ? err.message : "Sign in failed");
      setWiggle(true);
    } finally {
      setBusy(false);
    }
  }

  async function submitMfa(e: React.FormEvent) {
    e.preventDefault();
    if (!mfaToken) return;
    setBusy(true);
    setError("");
    setWiggle(false);
    try {
      const res = await fetch("/auth/sign_in", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(
          useBackup
            ? { mfa_token: mfaToken, backup_code: otpCode.trim() }
            : { mfa_token: mfaToken, otp_code: otpCode.trim() }
        ),
      });
      const me = await authResponse(res);
      nav(`/accounts/${me.account_id}/conversations`, { replace: true });
    } catch (err) {
      setError(err instanceof Error ? err.message : "Invalid verification code");
      setWiggle(true);
    } finally {
      setBusy(false);
    }
  }

  return (
    <AuthShell
      title={t("LOGIN.TITLE")}
      wiggle={wiggle}
      subtitle={
        <>
          {t("COMMON.OR")}{" "}
          <Link to="/auth/signup" className="text-n-brand hover:underline">
            {t("LOGIN.CREATE_NEW_ACCOUNT").toLowerCase()}
          </Link>
        </>
      }
    >
      {mfaToken ? (
        <form onSubmit={submitMfa} className="space-y-5">
          <div className="flex items-center gap-2 text-n-slate-12">
            <ShieldCheck className="size-5 text-n-brand" />
            <h2 className="text-base font-medium">Two-factor verification</h2>
          </div>
          <p className="text-sm text-n-slate-11">
            {useBackup
              ? "Enter one of your backup recovery codes."
              : "Enter the 6-digit code from your authenticator app."}
          </p>
          <label className="block space-y-1.5">
            <span className="text-sm font-medium text-n-slate-12">
              {useBackup ? "Backup code" : "Verification code"}
            </span>
            <input
              required
              autoFocus
              value={otpCode}
              onChange={(e) => setOtpCode(e.target.value)}
              placeholder={useBackup ? "xxxx-xxxx" : "123456"}
              className={inputCls}
            />
          </label>
          {error && (
            <p role="alert" className="text-sm text-n-ruby-11">
              {error}
            </p>
          )}
          <button
            type="submit"
            disabled={busy}
            className="flex h-10 w-full items-center justify-center rounded-lg bg-n-brand px-4 text-sm font-medium text-white transition hover:bg-n-blue-10 disabled:opacity-50"
          >
            {busy ? "Verifying..." : "Verify & Sign in"}
          </button>
          <div className="flex items-center justify-between text-xs text-n-slate-11">
            <button
              type="button"
              onClick={() => {
                setUseBackup((v) => !v);
                setOtpCode("");
                setError("");
              }}
              className="text-n-brand hover:underline"
            >
              {useBackup ? "Use authenticator code" : "Use backup code"}
            </button>
            <button
              type="button"
              onClick={() => {
                setMfaToken(null);
                setOtpCode("");
                setError("");
              }}
              className="hover:underline"
            >
              Back to login
            </button>
          </div>
        </form>
      ) : (
        <div className="space-y-5">
          {googleEnabled && (
            <a
              href="/omniauth/google_oauth2"
              className="inline-flex h-11 w-full items-center justify-center gap-2 rounded-lg bg-n-background px-4 text-sm font-medium text-n-slate-12 shadow-xs ring-1 ring-inset ring-n-weak transition hover:bg-n-alpha-2 dark:bg-n-solid-3"
            >
              <span>{t("LOGIN.OAUTH.GOOGLE_LOGIN") || "Sign in with Google"}</span>
            </a>
          )}
          <Link
            to="/login/sso"
            className="inline-flex h-11 w-full items-center justify-center gap-2 rounded-lg bg-n-background px-4 text-sm font-medium text-n-slate-12 shadow-xs ring-1 ring-inset ring-n-weak transition hover:bg-n-alpha-2 dark:bg-n-solid-3"
          >
            <LockKeyhole className="size-4 text-n-slate-11" />
            <span>Sign in via SAML SSO</span>
          </Link>

          <div className="relative flex items-center py-1">
            <div className="flex-grow border-t border-n-weak" />
            <span className="mx-3 shrink text-xs font-medium uppercase tracking-wider text-n-slate-10">
              {t("COMMON.OR")}
            </span>
            <div className="flex-grow border-t border-n-weak" />
          </div>

          <form onSubmit={submit} className="space-y-5">
            <label className="block space-y-1.5">
              <span className="text-sm font-medium text-n-slate-12">
                {t("LOGIN.EMAIL.LABEL")}
              </span>
              <input
                name="email"
                type="email"
                autoComplete="email"
                required
                autoFocus
                placeholder={t("LOGIN.EMAIL.PLACEHOLDER")}
                className={inputCls}
              />
            </label>

            <div className="space-y-1.5">
              <div className="flex items-center justify-between">
                <label
                  htmlFor="login-password"
                  className="text-sm font-medium text-n-slate-12"
                >
                  {t("LOGIN.PASSWORD.LABEL")}
                </label>
                <Link
                  className="text-xs font-medium text-n-brand hover:underline"
                  to="/auth/reset/password"
                >
                  {t("LOGIN.FORGOT_PASSWORD")}
                </Link>
              </div>
              <input
                id="login-password"
                name="password"
                type="password"
                autoComplete="current-password"
                required
                placeholder={t("LOGIN.PASSWORD.PLACEHOLDER")}
                className={inputCls}
              />
            </div>

            {error && (
              <p role="alert" className="text-sm text-n-ruby-11">
                {error}
              </p>
            )}

            <button
              type="submit"
              disabled={busy}
              className="flex h-10 w-full items-center justify-center rounded-lg bg-n-brand px-4 text-sm font-medium text-white shadow-xs transition hover:bg-n-blue-10 disabled:opacity-50"
            >
              {busy ? "Signing in..." : t("LOGIN.SUBMIT")}
            </button>
          </form>
        </div>
      )}
    </AuthShell>
  );
}
