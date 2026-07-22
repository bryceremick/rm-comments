// Package comment.
package main

/* block
   comment */
func main() {
	url := "https://example.com" // trailing
	s := "not // a comment"
	_, _ = url, s
}
