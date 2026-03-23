import { auth } from "$lib/forge";

export function getUserId(): string {
  const user = auth.user;
  if (!user?.id) throw new Error("Not authenticated");
  return user.id;
}
