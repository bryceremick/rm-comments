<?php
// Line comment.
# Hash comment.
/* block
   comment */
function main() {
    $url = "https://example.com"; // trailing
    $s = 'not // a comment';
    return [$url, $s];
}
