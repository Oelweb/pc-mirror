/* Erzeugt Vergleichswerte für den Rust-Port von playfair_decrypt.
 * Bauen (MSYS2 UCRT64): gcc -O0 -o fpv fairplay_vectors.c ../../../third_party/uxplay/lib/playfair/*.c
 */
#include <stdio.h>
#include <stdint.h>
void playfair_decrypt(unsigned char* message3, unsigned char* cipherText, unsigned char* keyOut);
static uint64_t state = 0x9E3779B97F4A7C15ull;
static unsigned char next_byte(void) {
    state ^= state << 13; state ^= state >> 7; state ^= state << 17;
    return (unsigned char) (state >> 24);
}
int main(void) {
    for (int n = 0; n < 100; n++) {
        unsigned char keymsg[164], ekey[72], out[16];
        for (int i = 0; i < 164; i++) keymsg[i] = next_byte();
        for (int i = 0; i < 72; i++) ekey[i] = next_byte();
        keymsg[4] = 3;
        keymsg[12] = (unsigned char) (n % 4);
        playfair_decrypt(keymsg, ekey, out);
        for (int i = 0; i < 164; i++) printf("%02x", keymsg[i]);
        printf(" ");
        for (int i = 0; i < 72; i++) printf("%02x", ekey[i]);
        printf(" ");
        for (int i = 0; i < 16; i++) printf("%02x", out[i]);
        printf("\n");
    }
    return 0;
}
