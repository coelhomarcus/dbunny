import type { Translation } from "../messages.ts";
import type { en } from "./en.ts";

export const ptBR: Translation<typeof en> = {
  common: {
    back: "Voltar",
    cancel: "Cancelar",
    close: "Fechar",
    copy: "Copiar",
    delete: "Excluir",
    retry: "Tentar de novo",
    save: "Salvar",
  },
  settings: {
    language: {
      label: "Idioma",
      hint: "O idioma do app. Sistema segue o do seu computador.",
      system: "Sistema",
    },
  },
};
