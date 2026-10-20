/** The requests the editor's actions send to the assistant. Written in English like the rest of what it is told. */

/** The statement in a fenced block, with a longer fence if it holds one of its own. */
function sqlBlock(sql: string): string {
  const fence = sql.includes("```") ? "````" : "```";
  return `${fence}sql\n${sql}\n${fence}`;
}

export function explainPrompt({ engine, sql }: { engine: string; sql: string }): string {
  return `Explain what this ${engine} query does, step by step, in plain words.\n\n${sqlBlock(sql)}`; // i18n-ignore: a prompt for the agent
}

export function optimizePrompt({ engine, sql, plan }: { engine: string; sql: string; plan?: string }): string {
  const intro =
    `Analyze the performance of this ${engine} query. Use explain_query to read its plan (ask before running it with analyze), ` + // i18n-ignore: a prompt for the agent
    "then suggest indexes or a rewrite, with the SQL in sql blocks.";
  const ran = plan ? `\n\nThe plan I just ran:\n\`\`\`\n${plan}\n\`\`\`` : "";
  return `${intro}${ran}\n\n${sqlBlock(sql)}`;
}

export function fixPrompt({ engine, sql, error }: { engine: string; sql: string; error: string }): string {
  return `This ${engine} query failed with:\n\n${error}\n\nFind the cause using the schema and suggest the corrected SQL in a sql block.\n\n${sqlBlock(sql)}`; // i18n-ignore: a prompt for the agent
}

/** What an error of the console says, as plain text: the console sends JSON, with the server's detail and hint. */
export function errorTextOf(raw: string): string {
  try {
    const parsed: unknown = JSON.parse(raw);
    if (parsed !== null && typeof parsed === "object" && typeof (parsed as { message?: unknown }).message === "string") {
      const { message, detail, hint } = parsed as { message: string; detail?: unknown; hint?: unknown };
      return [message, typeof detail === "string" && detail ? `Detail: ${detail}` : "", typeof hint === "string" && hint ? `Hint: ${hint}` : ""].filter(Boolean).join("\n");
    }
  } catch {
    // not JSON: the text is the message
  }
  return raw;
}
