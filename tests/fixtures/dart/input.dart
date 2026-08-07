/// Doc comment, kept by default.
library greeter;

// narration, removed
/* block
   comment */
void main() {
  var url = 'https://example.com'; // trailing
  var s = 'not // a comment';
  // ignore: unused_local_variable
  var t = s;
  // TODO: keep me
  print(url);
}
