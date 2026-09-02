# Simple explanations

Use simple words. Explain clearly. Do not be a smartass.

- Start from what the bytes / code actually do. Then add the name for it.
- Prefer a concrete example (byte 0, byte 1..) over a slogan ("layout is already clean", "neighbors sit flush").
- Do not stack jargon. If you need a term (Pod, repr(C), alignment), define it in one plain sentence the first time.
- Do not compress three ideas into one dense paragraph. One idea, then the next.
- If a sentence could mean two things, rewrite it. "The macro applies X if you do not" is bad. "You do not have to write X. The macro adds it" is good.

```
BAD:  Native integers are allowed when the layout is already clean.
GOOD: Counter can use a real u64 because that field starts at byte 0.
      A u8 then a u32 cannot, because u32 cannot start at byte 1.
```
