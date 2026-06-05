import { deleteKey } from "umari:crypto/keys@0.1.0";

/**
 * Permanently delete the encryption key associated with `scope`. This makes
 * all data encrypted under that scope unrecoverable ("crypto-shredding").
 *
 * Mirrors `crates/umari/src/runtime/crypto.rs::delete_crypto_key`.
 */
export function deleteCryptoKey(scope: string): void {
  deleteKey(scope);
}
