/* Block comment. */
#include <stdio.h>

// Line comment.
int main(void) {
    const char *url = "https://example.com"; // trailing
    const char *s = "not /* a comment */";
    printf("%s %s\n", url, s);
    return 0;
}
