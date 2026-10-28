import { invoke } from "./invoke";

/** Answers a `credentials-request` event; `null` cancels (git then fails as it did without a prompt). */
export const answerCredentials = (id: number, value: string | null) =>
  invoke<void>("answer_credentials", { id, value });
