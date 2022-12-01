use std::fmt::{Debug, Display};
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut, Index, IndexMut};
use std::slice::SliceIndex;

use super::super::{
    cons::Cons,
    object::{GcObj, RawObj},
};
use super::{Block, Context, RootSet, Trace};
use crate::core::env::{ConstSymbol, Symbol};
use crate::core::object::{ByteFn, Gc, IntoObject, LispString, Object, Untag, WithLifetime};
use crate::hashmap::{HashMap, HashSet};

pub(crate) trait IntoRoot<T> {
    unsafe fn into_root(self) -> T;
}

impl<T, U> IntoRoot<Gc<U>> for Gc<T>
where
    Gc<T>: WithLifetime<'static, Out = Gc<U>>,
    U: 'static,
{
    unsafe fn into_root(self) -> Gc<U> {
        self.with_lifetime()
    }
}

impl<T> IntoRoot<T> for &Root<'_, '_, T>
where
    T: Copy,
{
    unsafe fn into_root(self) -> T {
        *self.data
    }
}

impl<T, U> IntoRoot<U> for &Rt<T>
where
    T: WithLifetime<'static, Out = U>,
{
    unsafe fn into_root(self) -> U {
        self.inner.with_lifetime()
    }
}

impl IntoRoot<GcObj<'static>> for bool {
    unsafe fn into_root(self) -> GcObj<'static> {
        self.into()
    }
}

impl IntoRoot<GcObj<'static>> for i64 {
    unsafe fn into_root(self) -> GcObj<'static> {
        self.into()
    }
}

impl IntoRoot<&'static Cons> for &Cons {
    unsafe fn into_root(self) -> &'static Cons {
        self.with_lifetime()
    }
}

impl IntoRoot<&'static ByteFn> for &ByteFn {
    unsafe fn into_root(self) -> &'static ByteFn {
        self.with_lifetime()
    }
}

impl IntoRoot<&'static Symbol> for &Symbol {
    unsafe fn into_root(self) -> &'static Symbol {
        self.with_lifetime()
    }
}

impl IntoRoot<&'static Symbol> for ConstSymbol {
    unsafe fn into_root(self) -> &'static Symbol {
        self.with_lifetime()
    }
}

impl<T, Tx> IntoRoot<Option<Tx>> for Option<T>
where
    T: IntoRoot<Tx>,
{
    unsafe fn into_root(self) -> Option<Tx> {
        self.map(|x| x.into_root())
    }
}

impl<T, U, Tx, Ux> IntoRoot<(Tx, Ux)> for (T, U)
where
    T: IntoRoot<Tx>,
    U: IntoRoot<Ux>,
{
    unsafe fn into_root(self) -> (Tx, Ux) {
        (self.0.into_root(), self.1.into_root())
    }
}

impl<T: IntoRoot<U>, U> IntoRoot<Vec<U>> for Vec<T> {
    unsafe fn into_root(self) -> Vec<U> {
        self.into_iter().map(|x| x.into_root()).collect()
    }
}

impl<T> Trace for Gc<T> {
    fn trace(&self, stack: &mut Vec<RawObj>) {
        self.as_obj().trace_mark(stack);
    }
}

/// Represents a Rooted object T. The purpose of this type is we cannot have
/// mutable references to the inner data, because the garbage collector will
/// need to trace it. This type will only give us a mut [`Rt`] (rooted mutable
/// reference) when we are also holding a reference to the Context, meaning that
/// garbage collection cannot happen.
pub(crate) struct Root<'rt, 'a, T> {
    data: *mut T,
    root_set: &'rt RootSet,
    // This lifetime parameter ensures that functions like mem::swap cannot be
    // called in a way that would lead to memory unsafety. Since the drop guard
    // of Root is critical to ensure that T gets unrooted the same time it is
    // dropped, calling swap would invalidate this invariant.
    safety: PhantomData<&'a ()>,
}

impl<'rt, T> Root<'rt, '_, T> {
    pub(crate) unsafe fn new(root_set: &'rt RootSet) -> Self {
        Self {
            data: std::ptr::null_mut(),
            root_set,
            safety: PhantomData,
        }
    }

    pub(crate) fn as_mut<'a>(&'a mut self, _cx: &'a Context) -> &'a mut Rt<T> {
        // SAFETY: We have a reference to the Context
        unsafe { self.deref_mut_unchecked() }
    }

    pub(crate) unsafe fn deref_mut_unchecked(&mut self) -> &mut Rt<T> {
        assert!(
            !self.data.is_null(),
            "Attempt to mutably deref uninitialzed Root"
        );
        &mut *self.data.cast::<Rt<T>>()
    }
}

impl<T> Deref for Root<'_, '_, T> {
    type Target = Rt<T>;

    fn deref(&self) -> &Self::Target {
        assert!(!self.data.is_null(), "Attempt to deref uninitialzed Root");
        unsafe { &*self.data.cast::<Rt<T>>() }
    }
}

impl<T> AsRef<Rt<T>> for Root<'_, '_, T> {
    fn as_ref(&self) -> &Rt<T> {
        self
    }
}

impl<T: Debug> Debug for Root<'_, '_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&**self, f)
    }
}

impl<T: Display> Display for Root<'_, '_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&**self, f)
    }
}

impl<'rt, T: Trace + 'static> Root<'rt, '_, T> {
    pub(crate) unsafe fn init<'brw>(
        root: &'brw mut Self,
        data: &'brw mut T,
    ) -> &'brw mut Root<'rt, 'brw, T> {
        assert!(root.data.is_null(), "Attempt to reinit Root");
        let dyn_ptr = data as &mut dyn Trace as *mut dyn Trace;
        root.data = dyn_ptr.cast::<T>();
        root.root_set.roots.borrow_mut().push(dyn_ptr);
        // We need the safety lifetime to match the borrow
        std::mem::transmute::<&mut Root<'rt, '_, T>, &mut Root<'rt, 'brw, T>>(root)
    }
}

impl<T> Drop for Root<'_, '_, T> {
    fn drop(&mut self) {
        if self.data.is_null() {
            if std::thread::panicking() {
                eprintln!("Error: Root was dropped while still not set");
            } else {
                panic!("Error: Root was dropped while still not set");
            }
        } else {
            self.root_set.roots.borrow_mut().pop();
        }
    }
}

#[macro_export]
macro_rules! root {
    ($ident:ident, $cx:ident) => {
        root!(
            $ident,
            unsafe { $crate::core::gc::IntoRoot::into_root($ident) },
            $cx
        );
    };
    ($ident:ident, move($value:expr), $cx:ident) => {
        root!(
            $ident,
            unsafe { $crate::core::gc::IntoRoot::into_root($value) },
            $cx
        );
    };
    ($ident:ident, $value:expr, $cx:ident) => {
        let mut rooted = $value;
        let mut root: $crate::core::gc::Root<_> =
            unsafe { $crate::core::gc::Root::new($cx.get_root_set()) };
        let $ident = unsafe { $crate::core::gc::Root::init(&mut root, &mut rooted) };
    };
}

/// A Rooted type. If a type is wrapped in Rt, it is known to be rooted and hold
/// items past garbage collection. This type is never used as an owned type,
/// only a reference. This ensures that underlying data does not move. In order
/// to access the inner data, the [`Rt::bind`] method must be used.
#[repr(transparent)]
pub(crate) struct Rt<T: ?Sized> {
    inner: T,
}

impl<T: Debug> Debug for Rt<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self.inner, f)
    }
}

impl<T: Display> Display for Rt<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.inner, f)
    }
}

impl PartialEq for Rt<GcObj<'_>> {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl PartialEq for Rt<&Symbol> {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl Eq for Rt<&Symbol> {}

impl<T: PartialEq<U>, U> PartialEq<U> for Rt<T> {
    fn eq(&self, other: &U) -> bool {
        self.inner == *other
    }
}

impl Deref for Rt<Gc<&LispString>> {
    type Target = LispString;

    fn deref(&self) -> &Self::Target {
        self.inner.get()
    }
}

impl<T> Hash for Rt<T>
where
    T: Hash,
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.inner.hash(state);
    }
}

impl<'new, T> WithLifetime<'new> for Option<T>
where
    T: WithLifetime<'new>,
{
    type Out = Option<<T as WithLifetime<'new>>::Out>;

    unsafe fn with_lifetime(self) -> Self::Out {
        self.map(|x| x.with_lifetime())
    }
}

impl<'new, T, U> WithLifetime<'new> for (T, U)
where
    T: WithLifetime<'new>,
    U: WithLifetime<'new>,
{
    type Out = (
        <T as WithLifetime<'new>>::Out,
        <U as WithLifetime<'new>>::Out,
    );

    unsafe fn with_lifetime(self) -> Self::Out {
        (self.0.with_lifetime(), self.1.with_lifetime())
    }
}

impl<T> Rt<T> {
    pub(crate) fn bind<'ob>(&self, _: &'ob Context) -> <T as WithLifetime<'ob>>::Out
    where
        T: WithLifetime<'ob>,
    {
        // SAFETY: We are holding a reference to the context
        unsafe { self.inner.with_lifetime() }
    }

    pub(crate) unsafe fn bind_unchecked<'ob>(&'ob self) -> <T as WithLifetime<'ob>>::Out
    where
        T: WithLifetime<'ob>,
    {
        self.inner.with_lifetime()
    }

    pub(crate) fn bind_slice<'ob, U>(slice: &[Rt<T>], _: &'ob Context) -> &'ob [U]
    where
        T: WithLifetime<'ob, Out = U>,
    {
        unsafe { &*(slice as *const [Rt<T>] as *const [U]) }
    }

    /// This functions is very unsafe to call directly. The caller must ensure
    /// that resulting Rt is only exposed through references and that it is
    /// properly rooted.
    pub(crate) unsafe fn new_unchecked(item: T) -> Rt<T> {
        Rt { inner: item }
    }
}

impl TryFrom<&Rt<GcObj<'_>>> for usize {
    type Error = anyhow::Error;

    fn try_from(value: &Rt<GcObj>) -> Result<Self, Self::Error> {
        value.inner.try_into()
    }
}

impl<T> Rt<Gc<T>> {
    /// Like `try_into`, but needed to due no specialization
    pub(crate) fn try_into<U, E>(&self) -> Result<&Rt<Gc<U>>, E>
    where
        Gc<T>: TryInto<Gc<U>, Error = E> + Copy,
    {
        let _: Gc<U> = self.inner.try_into()?;
        // SAFETY: This is safe because all Gc types have the same representation
        unsafe { Ok(&*((self as *const Self).cast::<Rt<Gc<U>>>())) }
    }

    /// Like `try_into().bind(cx)`, but needed to due no specialization
    pub(crate) fn bind_as<'ob, U, E>(&self, _cx: &'ob Context) -> Result<U, E>
    where
        Gc<T>: TryInto<U, Error = E> + Copy,
        U: 'ob,
    {
        self.inner.try_into()
    }

    /// Like `From`, but needed to due no specialization
    pub(crate) fn use_as<U>(&self) -> &Rt<Gc<U>>
    where
        Gc<T>: Into<Gc<U>> + Copy,
    {
        // SAFETY: This is safe because all Gc types have the same representation
        unsafe { &*((self as *const Self).cast::<Rt<Gc<U>>>()) }
    }

    // TODO: Find a way to remove this method. We should never need to guess
    // if something is cons
    pub(crate) fn as_cons(&self) -> &Rt<Gc<&Cons>> {
        match self.inner.as_obj().get() {
            crate::core::object::Object::Cons(_) => unsafe {
                &*(self as *const Self).cast::<Rt<Gc<&Cons>>>()
            },
            x => panic!("attempt to convert type that was not cons: {x}"),
        }
    }

    pub(crate) fn get<'ob, U>(&self, cx: &'ob Context) -> U
    where
        Gc<T>: WithLifetime<'ob, Out = Gc<U>>,
        Gc<U>: Untag<U>,
    {
        let gc: Gc<U> = self.bind(cx);
        gc.untag()
    }

    pub(crate) fn set<U>(&mut self, item: U)
    where
        U: IntoRoot<Gc<T>>,
    {
        unsafe {
            self.inner = item.into_root();
        }
    }
}

impl From<&Rt<GcObj<'_>>> for Option<()> {
    fn from(value: &Rt<GcObj<'_>>) -> Self {
        value.inner.nil().then_some(())
    }
}

impl Rt<GcObj<'static>> {
    pub(crate) fn try_as_option<T, E>(&self) -> Result<Option<&Rt<Gc<T>>>, E>
    where
        GcObj<'static>: TryInto<Gc<T>, Error = E>,
    {
        if self.inner.nil() {
            Ok(None)
        } else {
            let _: Gc<T> = self.inner.try_into()?;
            unsafe { Ok(Some(&*((self as *const Self).cast::<Rt<Gc<T>>>()))) }
        }
    }
}

impl IntoObject for &Rt<GcObj<'static>> {
    type Out<'ob> = Object<'ob>;

    fn into_obj<const C: bool>(self, _block: &Block<C>) -> Gc<Self::Out<'_>> {
        unsafe { self.inner.with_lifetime() }
    }
}

impl IntoObject for &Root<'_, '_, GcObj<'static>> {
    type Out<'ob> = Object<'ob>;

    fn into_obj<const C: bool>(self, _block: &Block<C>) -> Gc<Self::Out<'_>> {
        unsafe { self.inner.with_lifetime() }
    }
}

impl IntoObject for &mut Root<'_, '_, GcObj<'static>> {
    type Out<'ob> = Object<'ob>;

    fn into_obj<const C: bool>(self, _block: &Block<C>) -> Gc<Self::Out<'_>> {
        unsafe { self.inner.with_lifetime() }
    }
}

impl Rt<&Cons> {
    pub(crate) fn set(&mut self, item: &Cons) {
        self.inner = unsafe { std::mem::transmute(item) }
    }

    pub(crate) fn car<'ob>(&self, cx: &'ob Context) -> GcObj<'ob> {
        self.bind(cx).car()
    }

    pub(crate) fn cdr<'ob>(&self, cx: &'ob Context) -> GcObj<'ob> {
        self.bind(cx).cdr()
    }
}

impl<T, U> Deref for Rt<(T, U)> {
    type Target = (Rt<T>, Rt<U>);

    fn deref(&self) -> &Self::Target {
        unsafe { &*(self as *const Self).cast::<(Rt<T>, Rt<U>)>() }
    }
}

impl<T, U> DerefMut for Rt<(T, U)> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *(self as *mut Rt<(T, U)>).cast::<(Rt<T>, Rt<U>)>() }
    }
}

impl<T> Deref for Rt<Option<T>> {
    type Target = Option<Rt<T>>;
    fn deref(&self) -> &Self::Target {
        unsafe { &*(self as *const Self).cast::<Self::Target>() }
    }
}

impl<T> DerefMut for Rt<Option<T>> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *(self as *mut Self).cast::<Self::Target>() }
    }
}

impl<T> Rt<Option<T>> {
    pub(crate) fn set<U: IntoRoot<T>>(&mut self, obj: U) {
        unsafe {
            self.inner = Some(obj.into_root());
        }
    }

    // This is not really dead code, but the static analysis fails to find it
    #[allow(dead_code)]
    pub(crate) fn as_ref(&self) -> Option<&Rt<T>> {
        let option = self.inner.as_ref();
        option.map(|x| unsafe { &*(x as *const T).cast::<Rt<T>>() })
    }
}

impl<T> Rt<Vec<T>> {
    // This is not safe to expose pub(crate)
    fn as_mut_ref(&mut self) -> &mut Vec<Rt<T>> {
        // SAFETY: `Rt<T>` has the same memory layout as `T`.
        unsafe { &mut *(self as *mut Self).cast::<Vec<Rt<T>>>() }
    }

    pub(crate) fn push<U: IntoRoot<T>>(&mut self, item: U) {
        self.inner.push(unsafe { item.into_root() });
    }

    pub(crate) fn truncate(&mut self, len: usize) {
        self.as_mut_ref().truncate(len);
    }

    pub(crate) fn pop(&mut self) {
        self.as_mut_ref().pop();
    }

    pub(crate) fn drain<R>(&mut self, range: R) -> std::vec::Drain<'_, Rt<T>>
    where
        R: std::ops::RangeBounds<usize>,
    {
        self.as_mut_ref().drain(range)
    }

    pub(crate) fn clear(&mut self) {
        self.as_mut_ref().clear();
    }

    pub(crate) fn swap_remove(&mut self, index: usize) {
        self.as_mut_ref().swap_remove(index);
    }

    pub(crate) fn pop_obj<'ob, U>(&mut self, _cx: &'ob Context) -> Option<U>
    where
        T: WithLifetime<'ob, Out = U>,
    {
        self.inner.pop().map(|x| unsafe { x.with_lifetime() })
    }
}

impl<T> Deref for Rt<Vec<T>> {
    type Target = [Rt<T>];
    fn deref(&self) -> &Self::Target {
        // SAFETY: `Rt<T>` has the same memory layout as `T`.
        let vec = unsafe { &*(self as *const Self).cast::<Vec<Rt<T>>>() };
        vec
    }
}

impl<T> DerefMut for Rt<Vec<T>> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: `Rt<T>` has the same memory layout as `T`.
        let vec = unsafe { &mut *(self as *mut Self).cast::<Vec<Rt<T>>>() };
        vec
    }
}

impl<T, I: SliceIndex<[Rt<T>]>> Index<I> for Rt<Vec<T>> {
    type Output = I::Output;

    fn index(&self, index: I) -> &Self::Output {
        let slice: &[Rt<T>] = self;
        Index::index(slice, index)
    }
}

impl<T, I: SliceIndex<[Rt<T>]>> IndexMut<I> for Rt<Vec<T>> {
    fn index_mut(&mut self, index: I) -> &mut Self::Output {
        IndexMut::index_mut(self.as_mut_ref(), index)
    }
}

impl<K, V> Rt<HashMap<K, V>>
where
    K: Eq + Hash,
{
    pub(crate) fn insert<Kx: IntoRoot<K>, Vx: IntoRoot<V>>(&mut self, k: Kx, v: Vx) {
        self.inner
            .insert(unsafe { k.into_root() }, unsafe { v.into_root() });
    }

    pub(crate) fn get<Q: IntoRoot<K>>(&self, k: Q) -> Option<&Rt<V>> {
        self.inner
            .get(unsafe { &k.into_root() })
            .map(|x| unsafe { &*(x as *const V).cast::<Rt<V>>() })
    }

    pub(crate) fn get_mut<Q: IntoRoot<K>>(&mut self, k: Q) -> Option<&mut Rt<V>> {
        self.inner
            .get_mut(unsafe { &k.into_root() })
            .map(|x| unsafe { &mut *(x as *mut V).cast::<Rt<V>>() })
    }

    pub(crate) fn remove<Q: IntoRoot<K>>(&mut self, k: Q) {
        self.inner.remove(unsafe { &k.into_root() });
    }
}

impl<K, V> Deref for Rt<HashMap<K, V>> {
    type Target = HashMap<Rt<K>, Rt<V>>;
    fn deref(&self) -> &Self::Target {
        // SAFETY: `Rt<T>` has the same memory layout as `T`.
        unsafe { &*(self as *const Self).cast::<Self::Target>() }
    }
}

impl<K, V> DerefMut for Rt<HashMap<K, V>> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: `Rt<T>` has the same memory layout as `T`.
        unsafe { &mut *(self as *mut Self).cast::<Self::Target>() }
    }
}

impl<T> Rt<HashSet<T>>
where
    T: Eq + Hash,
{
    pub(crate) fn insert<Tx: IntoRoot<T>>(&mut self, value: Tx) -> bool {
        self.inner.insert(unsafe { value.into_root() })
    }

    pub(crate) fn contains<Q: IntoRoot<T>>(&self, value: Q) -> bool {
        self.inner.contains(unsafe { &value.into_root() })
    }
}

impl<T> Deref for Rt<HashSet<T>> {
    type Target = HashSet<Rt<T>>;
    fn deref(&self) -> &Self::Target {
        // SAFETY: `Rt<T>` has the same memory layout as `T`.
        unsafe { &*(self as *const Self).cast::<Self::Target>() }
    }
}

impl<T> DerefMut for Rt<HashSet<T>> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: `Rt<T>` has the same memory layout as `T`.
        unsafe { &mut *(self as *mut Self).cast::<Self::Target>() }
    }
}

#[cfg(test)]
mod test {
    use crate::core::object::nil;

    use super::super::RootSet;
    use super::*;

    #[test]
    fn indexing() {
        let root = &RootSet::default();
        let cx = &Context::new(root);
        let mut vec: Rt<Vec<GcObj<'static>>> = Rt { inner: vec![] };

        vec.push(nil());
        assert_eq!(vec[0], nil());
        let str1 = cx.add("str1");
        let str2 = cx.add("str2");
        vec.push(str1);
        vec.push(str2);
        let slice = &vec[0..3];
        assert_eq!(vec![nil(), str1, str2], Rt::bind_slice(slice, cx));
    }
}
