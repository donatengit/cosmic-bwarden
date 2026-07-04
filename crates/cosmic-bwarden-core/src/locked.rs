use zeroize::Zeroize as _;

const LEN: usize = 4096;

static REGION_LOCK_WORKS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

/// RAII guard that `mlock`s a memory region for its lifetime, `munlock`ing
/// it on drop. Replaces the `region` crate, which is a thin wrapper around
/// the same `mlock`/`munlock` syscalls that `rustix` already exposes.
struct MlockGuard {
    ptr: *const u8,
    len: usize,
}

// The pointer is only ever passed to `mlock`/`munlock`; it's never
// dereferenced through this type, so it's safe to move/share across threads.
unsafe impl Send for MlockGuard {}
unsafe impl Sync for MlockGuard {}

impl MlockGuard {
    fn new(ptr: *const u8, len: usize) -> rustix::io::Result<Self> {
        unsafe { rustix::mm::mlock(ptr.cast_mut().cast(), len)? };
        Ok(Self { ptr, len })
    }
}

impl Drop for MlockGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = rustix::mm::munlock(self.ptr.cast_mut().cast(), self.len);
        }
    }
}

pub struct Vec {
    data: Box<arrayvec::ArrayVec<u8, LEN>>,
    _lock: Option<MlockGuard>,
}

impl Default for Vec {
    fn default() -> Self {
        let data = Box::new(arrayvec::ArrayVec::<_, LEN>::new());
        let lock = match REGION_LOCK_WORKS.get() {
            Some(true) => {
                // mlock worked before but can still fail here once the process
                // accumulates enough locked regions to hit RLIMIT_MEMLOCK
                // (64 KiB on older kernels). Degrade to an unlocked buffer with
                // a warning instead of panicking the secrets-holding daemon.
                match MlockGuard::new(data.as_ptr(), data.capacity()) {
                    Ok(lock) => Some(lock),
                    Err(e) => {
                        log::warn!(
                            "failed to lock memory region (RLIMIT_MEMLOCK exhausted?), \
                             continuing without mlock for this buffer: {e}"
                        );
                        None
                    }
                }
            }
            Some(false) => None,
            None => match MlockGuard::new(data.as_ptr(), data.capacity()) {
                Ok(lock) => {
                    let _ = REGION_LOCK_WORKS.set(true);
                    Some(lock)
                }
                Err(e) => {
                    if REGION_LOCK_WORKS.set(false).is_ok() {
                        eprintln!("failed to lock memory region: {e}");
                    }
                    None
                }
            },
        };
        Self { data, _lock: lock }
    }
}

impl Vec {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn data(&self) -> &[u8] {
        self.data.as_slice()
    }

    pub fn data_mut(&mut self) -> &mut [u8] {
        self.data.as_mut_slice()
    }

    pub fn zero(&mut self) {
        self.truncate(0);
        self.data.clear();
        for _ in 0..LEN {
            self.data.push(0);
        }
    }

    pub fn extend(&mut self, it: impl Iterator<Item = u8>) {
        self.data.extend(it);
    }

    pub fn truncate(&mut self, len: usize) {
        self.data.truncate(len);
    }
}

impl Drop for Vec {
    fn drop(&mut self) {
        self.zero();
        self.data.as_mut().zeroize();
    }
}

impl Clone for Vec {
    fn clone(&self) -> Self {
        let mut new_vec = Self::new();
        new_vec.extend(self.data().iter().copied());
        new_vec
    }
}

/// A session token (access/refresh). Held in an mlocked buffer so it can't
/// page to swap while the vault is unlocked (`[P1-4]` in docs/roadmap.md).
/// Tokens are JWTs of unbounded size; one that doesn't fit the fixed `LEN`
/// buffer degrades to a plain zeroize-on-drop heap `String` with a warning
/// (mirroring `Vec`'s own graceful degradation when `mlock` itself fails)
/// rather than panicking the secrets-holding daemon via `ArrayVec::extend`.
pub struct Token(TokenRepr);

#[derive(Clone)]
enum TokenRepr {
    Locked(Vec),
    Heap(String),
}

impl Token {
    pub fn from_string(s: &str) -> Self {
        if s.len() <= LEN {
            let mut v = Vec::new();
            v.extend(s.bytes());
            Self(TokenRepr::Locked(v))
        } else {
            log::warn!(
                "token of {} bytes exceeds the {LEN}-byte locked buffer; \
                 storing on the unlocked heap (zeroized on drop, but pageable)",
                s.len()
            );
            Self(TokenRepr::Heap(s.to_string()))
        }
    }

    pub fn expose(&self) -> &str {
        match &self.0 {
            // Invariant: only ever constructed from a &str, so the buffer is
            // always valid UTF-8.
            TokenRepr::Locked(v) => {
                std::str::from_utf8(v.data()).expect("locked token constructed from valid UTF-8")
            }
            TokenRepr::Heap(s) => s,
        }
    }
}

impl Clone for Token {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl Drop for Token {
    fn drop(&mut self) {
        // The Locked variant zeroizes in `Vec`'s own Drop.
        if let TokenRepr::Heap(s) = &mut self.0 {
            s.zeroize();
        }
    }
}

impl std::fmt::Debug for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "********")
    }
}

impl From<String> for Token {
    /// Consumes and zeroizes the source string, scrubbing the transient
    /// unlocked copy the token arrived in (HTTP response, keyring read).
    fn from(mut s: String) -> Self {
        let token = Self::from_string(&s);
        s.zeroize();
        token
    }
}

impl From<&str> for Token {
    fn from(s: &str) -> Self {
        Self::from_string(s)
    }
}

#[cfg(test)]
mod token_tests {
    use super::{Token, TokenRepr, LEN};

    #[test]
    fn roundtrips_and_uses_the_locked_buffer() {
        let jwt = "eyJhbGciOiJSUzI1NiJ9.payload.signature";
        let token = Token::from_string(jwt);
        assert_eq!(token.expose(), jwt);
        assert!(matches!(token.0, TokenRepr::Locked(_)));
    }

    #[test]
    fn boundary_size_still_fits_the_locked_buffer() {
        let s = "a".repeat(LEN);
        let token = Token::from_string(&s);
        assert_eq!(token.expose(), s);
        assert!(matches!(token.0, TokenRepr::Locked(_)));
    }

    // Regression guard: `ArrayVec::extend` panics past capacity — an
    // oversized JWT must degrade to the heap fallback, not abort the agent.
    #[test]
    fn oversized_token_degrades_to_heap_instead_of_panicking() {
        let s = "a".repeat(LEN + 1);
        let token = Token::from_string(&s);
        assert_eq!(token.expose(), s);
        assert!(matches!(token.0, TokenRepr::Heap(_)));
    }

    #[test]
    fn debug_never_prints_the_token() {
        let token = Token::from_string("super-secret-token");
        assert_eq!(format!("{token:?}"), "********");
    }

    #[test]
    fn from_string_zeroizes_the_source() {
        // Can't observe the freed allocation safely; assert the API shape —
        // From<String> takes ownership so the source can't linger in scope.
        let source = String::from("token-material");
        let token = Token::from(source);
        assert_eq!(token.expose(), "token-material");
    }

    #[test]
    fn clone_preserves_content_and_representation() {
        let token = Token::from_string("abc");
        let clone = token.clone();
        drop(token);
        assert_eq!(clone.expose(), "abc");
        assert!(matches!(clone.0, TokenRepr::Locked(_)));
    }
}

#[derive(Clone)]
pub struct Password {
    password: Vec,
}

impl Password {
    pub fn new(password: Vec) -> Self {
        Self { password }
    }

    pub fn from_string(s: &str) -> Self {
        let mut v = Vec::new();
        v.extend(s.as_bytes().iter().copied());
        Self::new(v)
    }

    pub fn password(&self) -> &[u8] {
        self.password.data()
    }
}

#[derive(Clone)]
pub struct Keys {
    keys: Vec,
}

impl Keys {
    pub fn new(keys: Vec) -> Self {
        Self { keys }
    }

    pub fn data(&self) -> &[u8] {
        self.keys.data()
    }

    pub fn enc_key(&self) -> &[u8] {
        &self.keys.data()[0..32]
    }

    pub fn mac_key(&self) -> &[u8] {
        &self.keys.data()[32..64]
    }
}

#[derive(Clone)]
pub struct PasswordHash {
    hash: Vec,
}

impl PasswordHash {
    pub fn new(hash: Vec) -> Self {
        Self { hash }
    }

    pub fn hash(&self) -> &[u8] {
        self.hash.data()
    }
}

#[derive(Clone)]
pub struct PrivateKey {
    private_key: Vec,
}

impl PrivateKey {
    pub fn new(private_key: Vec) -> Self {
        Self { private_key }
    }

    pub fn private_key(&self) -> &[u8] {
        self.private_key.data()
    }
}

#[derive(Clone)]
pub struct ApiKey {
    client_id: Password,
    client_secret: Password,
}

impl ApiKey {
    pub fn new(client_id: Password, client_secret: Password) -> Self {
        Self {
            client_id,
            client_secret,
        }
    }

    pub fn client_id(&self) -> &[u8] {
        self.client_id.password()
    }

    pub fn client_secret(&self) -> &[u8] {
        self.client_secret.password()
    }
}
