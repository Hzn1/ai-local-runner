import { describe, expect, it } from 'vitest';
import en from '../locales/en.json';
import ptBR from '../locales/pt-BR.json';

type Tree = { [key: string]: string | Tree };

function flatten(tree: Tree, prefix = ''): Map<string, string> {
  const output = new Map<string, string>();
  for (const [key, value] of Object.entries(tree)) {
    const path = prefix ? `${prefix}.${key}` : key;
    if (typeof value === 'string') {
      output.set(path, value);
    } else {
      for (const [child, text] of flatten(value, path)) {
        output.set(child, text);
      }
    }
  }
  return output;
}

function placeholders(text: string): string[] {
  return [...text.matchAll(/\{([a-zA-Z0-9_]+)\}/g)]
    .map((match) => match[1])
    .sort();
}

const english = flatten(en as Tree);
const portuguese = flatten(ptBR as Tree);

describe('locales', () => {
  it('have exactly the same keys', () => {
    expect([...portuguese.keys()].sort()).toEqual([...english.keys()].sort());
  });

  it('have no empty translations', () => {
    for (const [key, value] of [...english, ...portuguese]) {
      expect(value.trim(), key).not.toBe('');
    }
  });

  it('use the same interpolation placeholders', () => {
    for (const [key, value] of english) {
      expect(placeholders(portuguese.get(key) ?? ''), key).toEqual(
        placeholders(value),
      );
    }
  });
});
