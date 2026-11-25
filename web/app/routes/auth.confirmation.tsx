import { useEffect, useState } from "react";
import { Link, useSearchParams } from "react-router";
import { Button, ErrorText } from "~/components/ui";
import { api } from "~/lib/api";

export default function ConfirmationRoute() {
  const [params] = useSearchParams();
  const token = params.get("confirmation_token") ?? "";
  const [status, setStatus] = useState(token ? "Confirming your email..." : "Missing confirmation token.");
  const [error, setError] = useState("");
  const [done, setDone] = useState(false);

  useEffect(() => {
    if (!token) return;
    let alive = true;
    api("/auth/confirmation", {
      method: "POST",
      json: { confirmation_token: token },
    })
      .then(() => {
        if (!alive) return;
        setDone(true);
        setStatus("Your email has been confirmed. You can now sign in.");
      })
      .catch((e) => {
        if (!alive) return;
        setError(e instanceof Error ? e.message : "Confirmation failed");
        setStatus("");
      });
    return () => {
      alive = false;
    };
  }, [token]);

  return (
    <main className="grid min-h-dvh place-items-center px-6">
      <div className="w-full max-w-sm rounded-xl border border-line bg-surface p-6">
        <h1 className="text-xl font-medium text-ink">Email confirmation</h1>
        {status && <p role="status" className="mt-3 text-sm text-ink-2">{status}</p>}
        <ErrorText>{error}</ErrorText>
        <div className="mt-6">
          <Link to="/login">
            <Button className="w-full justify-center">{done ? "Go to sign in" : "Back to login"}</Button>
          </Link>
        </div>
      </div>
    </main>
  );
}
