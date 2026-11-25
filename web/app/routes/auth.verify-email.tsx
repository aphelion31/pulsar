import { useState } from "react";
import { Link } from "react-router";
import { Button, ErrorText, Input } from "~/components/ui";
import { api } from "~/lib/api";

export default function VerifyEmailRoute() {
  const [email, setEmail] = useState("");
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  async function resend(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError("");
    setMessage("");
    try {
      await api("/resend_confirmation", {
        method: "POST",
        json: { email: email.trim() },
      });
      setMessage("Confirmation instructions sent if that email exists.");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to send confirmation");
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="grid min-h-dvh place-items-center px-6">
      <div className="w-full max-w-sm">
        <p className="text-[15px] font-semibold tracking-tight">Pulsar</p>
        <h1 className="mt-8 text-2xl font-medium">Verify your email</h1>
        <p className="mt-2 text-ink-2">Enter your email address to resend the confirmation link.</p>
        <form onSubmit={resend} className="mt-8 grid gap-4">
          <Input label="Email" type="email" value={email} onChange={(e) => setEmail(e.target.value)} required autoFocus />
          <ErrorText>{error}</ErrorText>
          {message && <p role="status" className="text-sm text-ink-2">{message}</p>}
          <Button type="submit" disabled={busy} className="h-9 justify-center">
            {busy ? "Sending..." : "Resend confirmation email"}
          </Button>
        </form>
        <p className="mt-4 text-sm text-ink-2">
          <Link className="underline" to="/login">
            Back to sign in
          </Link>
        </p>
      </div>
    </main>
  );
}
