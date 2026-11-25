/**
 * The login page refuses names the server's `validate_user_name` would, so a
 * user never reaches the hub holding an account name no server accepts.
 */
import { describe, expect, it } from 'vitest';
import { isValidUserName } from './constants';

describe('isValidUserName', () => {
  it('accepts letters, digits, spaces, dashes and underscores', () => {
    for (const name of ['Lena', 'max_99', 'Anna-Lena K', 'Jörg', '名前']) {
      expect(isValidUserName(name), name).toBe(true);
    }
  });

  it('rejects what the server rejects', () => {
    for (const name of ['', ' ', ' Lena', 'Lena ', 'Lena 🌸', 'a.b', 'x'.repeat(33), 'ö'.repeat(17)]) {
      expect(isValidUserName(name), name).toBe(false);
    }
  });
});
