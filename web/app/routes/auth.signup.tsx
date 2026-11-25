import { useState } from "react";
import { Link, useNavigate } from "react-router";
import { Button, ErrorText, Input } from "~/components/ui";
import { authResponse } from "~/lib/api";

export default function SignupRoute() {
  const navigate = useNavigate();
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState(false);

  async function submit(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const f = new FormData(e.currentTarget);
    const password = String(f.get("password") ?? "");
    const confirm = String(f.get("password_confirmation") ?? "");
    if (password !== confirm) {
      setError("Passwords do not match");
      return;
    }
    setBusy(true);
    setError("");
    setMessage("");
    try {
      const res = await fetch("/api/v1/accounts", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          account_name: String(f.get("account_name") ?? ""),
          user_full_name: String(f.get("user_full_name") ?? ""),
          email: String(f.get("email") ?? ""),
          password,
        }),
      });
      if (res.headers.get("access-token")) {
        const profile = await authResponse(res);
        navigate(`/accounts/${profile.account_id}/conversations`, { replace: true });
        return;
      }
      const data = await res.json().catch(() => ({}));
      if (!res.ok) throw new Error(data.message || data.error || "Sign up failed");
      setMessage("Account created. Check your email to confirm your account or sign in.");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Sign up failed");
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="grid min-h-dvh place-items-center px-6">
      <div className="w-full max-w-sm">
        <p className="text-[15px] font-semibold tracking-tight">Pulsar</p>
        <h1 className="mt-8 text-2xl font-medium">Create your account</h1>
        <p className="mt-2 text-ink-2">Start managing customer conversations</p>
        <form onSubmit={submit} className="mt-8 grid gap-4">
          <Input label="Full name" name="user_full_name" required autoFocus />
          <Input label="Company / Account name" name="account_name" required />
          <Input label="Work email" name="email" type="email" autoComplete="email" required />
          <Input label="Password" name="password" type="password" minLength={6} required />
          <Input label="Confirm password" name="password_confirmation" type="password" minLength={6} required />
          <ErrorText>{error}</ErrorText>
          {message && <p role="status" className="text-sm text-ink-2">{message}</p>}
          <Button type="submit" disabled={busy} className="mt-2 h-9 justify-center">
            {busy ? "Creating account..." : "Create account"}
          </Button>
        </form>
        <p className="mt-4 text-sm text-ink-2">
          Already have an account?{" "}
          <Link className="underline" to="/login">
            Sign in
          </Link>
        </p>
      </div>
    </main>
  );
}
