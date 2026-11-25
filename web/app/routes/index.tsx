import { redirect } from "react-router";
import { api, getSession, type Profile } from "~/lib/api";

export async function clientLoader() {
  if (!getSession()) throw redirect("/login");
  const me = await api<Profile>("/api/v1/profile");
  throw redirect(`/accounts/${me.account_id}/conversations`);
}

export default function Index() {
  return null;
}
