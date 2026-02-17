export function getUserId(): string {
  const raw = localStorage.getItem("trellix_user");
  if (!raw) throw new Error("Not authenticated");
  const parsed = JSON.parse(raw) as { id?: string };
  if (!parsed.id) throw new Error("Not authenticated");
  return parsed.id;
}
