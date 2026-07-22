#!/usr/bin/env python3
# A line comment.


def main():
    """Docstring: NOT a comment, must survive."""
    url = "https://example.com/#anchor"  # trailing comment
    s = "not # a comment"
    # full-line comment
    return url, s
