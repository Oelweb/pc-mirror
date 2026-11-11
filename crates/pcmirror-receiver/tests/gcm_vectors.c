/* AES-128-GCM mit 16-Byte-IV wie in UxPlays crypto.c (gcm_encrypt), als Vergleichswert.
 * Bauen (MSYS2 UCRT64): gcc -o gcmv gcm_vectors.c -lcrypto */
#include <stdio.h>
#include <openssl/evp.h>
int main(void) {
    unsigned char key[16], iv[16], pt[32], ct[32], tag[16];
    for (int i = 0; i < 16; i++) { key[i] = (unsigned char) (i * 3 + 1); iv[i] = (unsigned char) (i * 5 + 2); }
    for (int i = 0; i < 32; i++) pt[i] = (unsigned char) (i * 7 + 3);
    EVP_CIPHER_CTX *ctx = EVP_CIPHER_CTX_new();
    int len = 0;
    EVP_EncryptInit_ex(ctx, EVP_aes_128_gcm(), NULL, NULL, NULL);
    EVP_CIPHER_CTX_ctrl(ctx, EVP_CTRL_GCM_SET_IVLEN, 16, NULL);
    EVP_EncryptInit_ex(ctx, NULL, NULL, key, iv);
    EVP_EncryptUpdate(ctx, ct, &len, pt, 32);
    EVP_EncryptFinal_ex(ctx, ct + len, &len);
    EVP_CIPHER_CTX_ctrl(ctx, EVP_CTRL_GCM_GET_TAG, 16, tag);
    for (int i = 0; i < 32; i++) printf("%02x", ct[i]);
    printf(" ");
    for (int i = 0; i < 16; i++) printf("%02x", tag[i]);
    printf("\n");
    return 0;
}
