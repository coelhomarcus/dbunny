/**
 * The changelog edited on the release. The updater's latest.json is written at build
 * time, before those notes are filled in, so it only has the download table.
 */
const REPO = "coelhomarcus/dbunny";

export async function fetchReleaseNotes(version: string): Promise<string> {
  const tag = version.startsWith("v") ? version : `v${version}`;
  const response = await fetch(`https://api.github.com/repos/${REPO}/releases/tags/${tag}`, {
    headers: { Accept: "application/vnd.github+json" },
  });
  if (!response.ok) {
    throw new Error(response.status === 404 ? "This release has no notes." : "Couldn't load the release notes.");
  }
  const data: unknown = await response.json();
  const body = data && typeof data === "object" && "body" in data ? data.body : null;
  if (typeof body !== "string" || body.trim() === "") throw new Error("This release has no notes.");
  return body;
}
