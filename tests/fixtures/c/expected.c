#include <stdio.h>

int main(void) {
    const char *url = "https://example.com";
    const char *s = "not /* a comment */";
    printf("%s %s\n", url, s);
    return 0;
}
