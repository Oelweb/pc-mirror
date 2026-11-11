/* Erzeugt AAC-ELD-Frames (44.1 kHz, Stereo, 480 Samples) als [2-Byte-Länge BE][Frame]... */
#include <stdio.h>
#include <math.h>
#include <fdk-aac/aacenc_lib.h>
int main(void) {
    HANDLE_AACENCODER enc;
    if (aacEncOpen(&enc, 0, 2) != AACENC_OK) return 1;
    aacEncoder_SetParam(enc, AACENC_AOT, 39);
    aacEncoder_SetParam(enc, AACENC_SAMPLERATE, 44100);
    aacEncoder_SetParam(enc, AACENC_CHANNELMODE, MODE_2);
    aacEncoder_SetParam(enc, AACENC_GRANULE_LENGTH, 480);
    aacEncoder_SetParam(enc, AACENC_TRANSMUX, TT_MP4_RAW);
    aacEncoder_SetParam(enc, AACENC_SBR_MODE, 0);
    aacEncoder_SetParam(enc, AACENC_BITRATE, 128000);
    if (aacEncEncode(enc, NULL, NULL, NULL, NULL) != AACENC_OK) return 2;
    AACENC_InfoStruct info; aacEncInfo(enc, &info);
    fprintf(stderr, "ASC:"); for (UINT i = 0; i < info.confSize; i++) fprintf(stderr, "%02x", info.confBuf[i]); fprintf(stderr, " frame=%u\n", info.frameLength);
    FILE *out = fopen("clip.eld", "wb");
    short pcm[480 * 2]; unsigned char buf[2048]; int frames = 0; double t = 0;
    for (int n = 0; n < 200; n++) {
        for (int i = 0; i < 480; i++) { short v = (short) (8000 * sin(t)); t += 2 * 3.14159265 * 440 / 44100; pcm[2*i] = v; pcm[2*i+1] = v; }
        void *ib = pcm; INT isz = sizeof(pcm), iel = 2, iid = IN_AUDIO_DATA; INT ins = 480 * 2;
        void *ob = buf; INT osz = sizeof(buf), oel = 1, oid = OUT_BITSTREAM_DATA;
        AACENC_BufDesc in = {1, &ib, &iid, &isz, &iel}, ou = {1, &ob, &oid, &osz, &oel};
        AACENC_InArgs ia = {ins, 0}; AACENC_OutArgs oa = {0};
        if (aacEncEncode(enc, &in, &ou, &ia, &oa) != AACENC_OK) return 3;
        if (oa.numOutBytes > 0) { unsigned char h[2] = {oa.numOutBytes >> 8, oa.numOutBytes & 255}; fwrite(h, 1, 2, out); fwrite(buf, 1, oa.numOutBytes, out); frames++; }
    }
    fclose(out); fprintf(stderr, "frames=%d first=%02x\n", frames, 0);
    return 0;
}
