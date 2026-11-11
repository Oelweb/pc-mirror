/* Erzeugt Vergleichswerte für den Rust-Port der SRP-Kopplung aus UxPlays srp.c.
 * Bauen (MSYS2 UCRT64): gcc -w -I../../../third_party/uxplay/lib -o srpv srp_vectors.c -lcrypto
 */
#include "srp.c"
static void hex(const char *name, const unsigned char *d, int n) {
    printf("%s=", name);
    for (int i = 0; i < n; i++) printf("%02x", d[i]);
    printf("\n");
}
int main(void) {
    const char *user = "AA:BB:CC:DD:EE:FF";
    const unsigned char *s = NULL, *v = NULL, *B = NULL;
    int len_s = 0, len_v = 0, len_B = 0;
    do {
        srp_create_salted_verification_key(SRP_SHA1, SRP_NG_2048, user, (const unsigned char *) "4711", 4,
                                           &s, &len_s, &v, &len_v, NULL, NULL);
    } while (len_s != 16);
    unsigned char b[32], a[32];
    for (int i = 0; i < 32; i++) { b[i] = (unsigned char) (i * 7 + 3); a[i] = (unsigned char) (i * 13 + 5); }
    NGConstant *ng = new_ng(SRP_NG_2048, NULL, NULL);
    BN_CTX *ctx = BN_CTX_new();
    BIGNUM *abn = BN_bin2bn(a, 32, NULL), *A = BN_new();
    BN_mod_exp(A, ng->g, abn, ng->N, ctx);
    unsigned char Abytes[256]; int len_A = BN_bn2bin(A, Abytes);
    struct SRPVerifier *ver = srp_verifier_new(SRP_SHA1, SRP_NG_2048, user, s, len_s, v, len_v, Abytes, len_A,
                                               b, 32, &B, &len_B, NULL, NULL, 1);
    hex("s", s, len_s);
    hex("b", b, 32);
    hex("A", Abytes, len_A);
    hex("B", B, len_B);
    hex("M", ver->M, 20);
    hex("HAMK", ver->H_AMK, 20);
    hex("K", ver->session_key, 40);
    return 0;
}
