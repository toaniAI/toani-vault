import { Entry } from "@napi-rs/keyring";

export const KEYCHAIN_SERVICE = "toani-vault-cli";
export const KEYCHAIN_ACCOUNT = "default";

function createEntry(): Entry {
  return new Entry(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT);
}

export const keychain = {
  set(token: string): void {
    createEntry().setPassword(token);
  },
  get(): string | null {
    try {
      return createEntry().getPassword();
    } catch {
      return null;
    }
  },
  delete(): boolean {
    try {
      createEntry().deletePassword();
      return true;
    } catch {
      return false;
    }
  },
};

