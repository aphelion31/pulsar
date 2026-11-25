import { t } from "~/lib/i18n";
import { clearSession } from "~/lib/api";

// mirrors chatwoot/app/javascript/dashboard/routes/dashboard/settings/ (no-accounts, account suspended)
function Frame({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="grid min-h-dvh place-items-center bg-n-background p-8 text-n-slate-12">
      <div className="max-w-md text-center">
        <h1 className="text-xl font-medium">{title}</h1>
        <div className="mt-3 text-sm text-n-slate-11">{children}</div>
      </div>
    </div>
  );
}

export function NoAccounts() {
  return (
    <Frame title={t("APP_GLOBAL.NO_ACCOUNTS.TITLE")}>
      <p>{t("APP_GLOBAL.NO_ACCOUNTS.MESSAGE_SELF_HOSTED")}</p>
      <button
        type="button"
        className="mt-6 rounded-lg border border-n-weak px-4 py-2 text-sm"
        onClick={() => {
          clearSession();
          location.assign("/app/login");
        }}
      >
        {t("APP_GLOBAL.NO_ACCOUNTS.LOGOUT")}
      </button>
    </Frame>
  );
}

export function AccountSuspended() {
  return (
    <Frame title={t("APP_GLOBAL.ACCOUNT_SUSPENDED.TITLE")}>
      <p>{t("APP_GLOBAL.ACCOUNT_SUSPENDED.MESSAGE")}</p>
    </Frame>
  );
}
