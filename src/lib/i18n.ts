import { addMessages, init, locale, waitLocale } from 'svelte-i18n';
import { derived } from 'svelte/store';
import en from '../locales/en.json';
import ptBR from '../locales/pt-BR.json';
import { setFormatLocale } from './format';

export const supportedLocales = ['en', 'pt-BR'] as const;
export type AppLocale = (typeof supportedLocales)[number];

let initialized = false;

export async function setupI18n(initial: AppLocale = 'en'): Promise<void> {
  if (!initialized) {
    addMessages('en', en);
    addMessages('pt-BR', ptBR);
    init({ fallbackLocale: 'en', initialLocale: initial });
    initialized = true;
  }
  await setAppLocale(initial);
}

export async function setAppLocale(next: AppLocale): Promise<void> {
  setFormatLocale(next);
  document.documentElement.lang = next;
  await locale.set(next);
  await waitLocale();
}

export function applyLanguageToDocument(): void {
  document.documentElement.lang = currentLocale();
}

function currentLocale(): string {
  let value = 'en';
  locale.subscribe((next) => {
    if (next) value = next;
  })();
  return value;
}

export const isPortuguese = derived(locale, ($locale) => $locale === 'pt-BR');
