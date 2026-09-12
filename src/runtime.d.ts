// Bun 1.4 implements Iterator.concat before TypeScript includes its declaration.
interface IteratorConstructor {
  concat: <T>(...iterables: Iterable<T>[]) => IteratorObject<T, undefined>;
}
