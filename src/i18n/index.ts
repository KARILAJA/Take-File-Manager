import idDict from "./id.json";
import enDict from "./en.json";

export type Language = "id" | "en";
export type TranslationKey = keyof typeof idDict;

export const dictionaries = {
  id: idDict,
  en: enDict,
};

export function translate(lang: Language, key: TranslationKey): string {
  return dictionaries[lang][key] || dictionaries.en[key] || key;
}
