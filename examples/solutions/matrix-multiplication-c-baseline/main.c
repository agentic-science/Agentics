#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int read_exact(FILE *file, void *buffer, size_t bytes) {
    return fread(buffer, 1, bytes, file) == bytes ? 0 : 1;
}

static int write_exact(FILE *file, const void *buffer, size_t bytes) {
    return fwrite(buffer, 1, bytes, file) == bytes ? 0 : 1;
}

static void matmul(const float *a, const float *b, float *c, uint32_t m, uint32_t k, uint32_t n) {
    for (uint32_t row = 0; row < m; row++) {
        for (uint32_t col = 0; col < n; col++) {
            float acc = 0.0f;
            for (uint32_t inner = 0; inner < k; inner++) {
                acc += a[(size_t)row * k + inner] * b[(size_t)inner * n + col];
            }
            c[(size_t)row * n + col] = acc;
        }
    }
}

int main(int argc, char **argv) {
    if (argc != 3) {
        fprintf(stderr, "usage: %s INPUT OUTPUT\n", argv[0]);
        return 2;
    }

    FILE *input = fopen(argv[1], "rb");
    if (input == NULL) {
        perror("open input");
        return 1;
    }

    FILE *output = fopen(argv[2], "wb");
    if (output == NULL) {
        perror("open output");
        fclose(input);
        return 1;
    }

    char magic[8];
    uint32_t cases = 0;
    uint32_t m = 0;
    uint32_t k = 0;
    uint32_t n = 0;
    if (read_exact(input, magic, sizeof(magic)) || memcmp(magic, "AGMMIN1", 7) != 0 ||
        read_exact(input, &cases, sizeof(cases)) || read_exact(input, &m, sizeof(m)) ||
        read_exact(input, &k, sizeof(k)) || read_exact(input, &n, sizeof(n))) {
        fprintf(stderr, "invalid input header\n");
        fclose(input);
        fclose(output);
        return 1;
    }

    const char output_magic[8] = {'A', 'G', 'M', 'M', 'O', 'U', 'T', '1'};
    if (write_exact(output, output_magic, sizeof(output_magic)) ||
        write_exact(output, &cases, sizeof(cases)) || write_exact(output, &m, sizeof(m)) ||
        write_exact(output, &n, sizeof(n))) {
        fprintf(stderr, "failed to write output header\n");
        fclose(input);
        fclose(output);
        return 1;
    }

    size_t a_len = (size_t)m * k;
    size_t b_len = (size_t)k * n;
    size_t c_len = (size_t)m * n;
    float *a = calloc(a_len, sizeof(float));
    float *b = calloc(b_len, sizeof(float));
    float *c = calloc(c_len, sizeof(float));
    if (a == NULL || b == NULL || c == NULL) {
        fprintf(stderr, "allocation failed\n");
        free(a);
        free(b);
        free(c);
        fclose(input);
        fclose(output);
        return 1;
    }

    for (uint32_t case_index = 0; case_index < cases; case_index++) {
        if (read_exact(input, a, a_len * sizeof(float)) ||
            read_exact(input, b, b_len * sizeof(float))) {
            fprintf(stderr, "truncated input case\n");
            free(a);
            free(b);
            free(c);
            fclose(input);
            fclose(output);
            return 1;
        }

        matmul(a, b, c, m, k, n);
        if (write_exact(output, c, c_len * sizeof(float))) {
            fprintf(stderr, "failed to write output case\n");
            free(a);
            free(b);
            free(c);
            fclose(input);
            fclose(output);
            return 1;
        }
    }

    free(a);
    free(b);
    free(c);
    fclose(input);
    fclose(output);
    return 0;
}
