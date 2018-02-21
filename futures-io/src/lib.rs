//! Asynchronous IO
//!
//! This crate contains the `AsyncRead` and `AsyncWrite` traits which allow
//! data to be read and written asynchronously.

#![no_std]
#![deny(missing_docs, missing_debug_implementations)]
#![doc(html_root_url = "https://docs.rs/futures-io/0.2")]

macro_rules! if_std {
    ($($i:item)*) => ($(
        #[cfg(feature = "std")]
        $i
    )*)
}

if_std! {
    extern crate futures_core;
    extern crate iovec;
    extern crate std;

    use futures_core::{Async, Poll, task};
    use std::boxed::Box;
    use std::io as StdIo;
    use std::ptr;
    use std::vec::Vec;

    // Re-export IoVec for convenience
    pub use iovec::IoVec;

    // Re-export io::Error so that users don't have to deal
    // with conflicts when `use`ing `futures::io` and `std::io`.
    pub use StdIo::Error as Error;

    /// A type used to conditionally initialize buffers passed to `AsyncRead`
    /// methods.
    #[derive(Debug)]
    pub struct Initializer(bool);

    impl Initializer {
        /// Returns a new `Initializer` which will zero out buffers.
        #[inline]
        pub fn zeroing() -> Initializer {
            Initializer(true)
        }

        /// Returns a new `Initializer` which will not zero out buffers.
        ///
        /// # Safety
        ///
        /// This method may only be called by `AsyncRead`ers which guarantee
        /// that they will not read from the buffers passed to `AsyncRead`
        /// methods, and that the return value of the method accurately reflects
        /// the number of bytes that have been written to the head of the buffer.
        #[inline]
        pub unsafe fn nop() -> Initializer {
            Initializer(false)
        }

        /// Indicates if a buffer should be initialized.
        #[inline]
        pub fn should_initialize(&self) -> bool {
            self.0
        }

        /// Initializes a buffer if necessary.
        #[inline]
        pub fn initialize(&self, buf: &mut [u8]) {
            if self.should_initialize() {
                unsafe { ptr::write_bytes(buf.as_mut_ptr(), 0, buf.len()) }
            }
        }
    }

    /// Objects which can be read asynchronously.
    pub trait AsyncRead {
        /// Determines if this `AsyncRead`er can work with buffers of
        /// uninitialized memory.
        ///
        /// The default implementation returns an initializer which will zero
        /// buffers.
        ///
        /// # Safety
        ///
        /// This method is `unsafe` because and `AsyncRead`er could otherwise
        /// return a non-zeroing `Initializer` from another `AsyncRead` type
        /// without an `unsafe` block.
        #[inline]
        unsafe fn initializer(&self) -> Initializer {
            Initializer::zeroing()
        }

        /// Attempt to read from the `AsyncRead` into `buf`.
        ///
        /// On success, returns `Ok(Async::Ready(num_bytes_read))`.
        ///
        /// If reading would block, this function returns `Ok(Async::Pending)`
        /// and arranges for `cx.waker()` to receive a notification when the
        /// object becomes readable or is closed.
        fn poll_read(&mut self, cx: &mut task::Context, buf: &mut [u8])
            -> Poll<usize, Error>;

        /// Attempt to read from the `AsyncRead` into `vec` using vectored
        /// IO operations. This allows data to be read into multiple buffers
        /// using a single operation.
        ///
        /// On success, returns `Ok(Async::Ready(num_bytes_read))`.
        ///
        /// By default, this method delegates to using `poll_read` on the first
        /// buffer in `vec`. Objects which support vectored IO should override
        /// this method.
        ///
        /// If reading would block, this function returns `Ok(Async::Pending)`
        /// and arranges for `cx.waker()` to receive a notification when the
        /// object becomes readable or is closed.
        fn poll_vectored_read(&mut self, cx: &mut task::Context, vec: &mut [&mut IoVec])
            -> Poll<usize, Error>
        {
            if let Some(ref mut first_iovec) = vec.get_mut(0) {
                self.poll_read(cx, first_iovec)
            } else {
                // `vec` is empty.
                return Ok(Async::Ready(0));
            }
        }
    }

    /// Objects which can be written to asynchronously.
    pub trait AsyncWrite {
        /// Attempt to write bytes from `buf` into the object.
        ///
        /// On success, returns `Ok(Async::Ready(num_bytes_written))`.
        ///
        /// If writing would block, this function returns `Ok(Async::Pending)`
        /// and arranges for `cx.waker()` to receive a notification when the
        /// the object becomes writable or is closed.
        fn poll_write(&mut self, cx: &mut task::Context, buf: &[u8])
            -> Poll<usize, Error>;

        /// Attempt to write bytes from `vec` into the object using vectored
        /// IO operations. This allows data from multiple buffers to be written
        /// using a single operation.
        ///
        /// On success, returns `Ok(Async::Ready(num_bytes_written))`.
        ///
        /// By default, this method delegates to using `poll_write` on the first
        /// buffer in `vec`. Objects which support vectored IO should override
        /// this method.
        ///
        /// If writing would block, this function returns `Ok(Async::Pending)`
        /// and arranges for `cx.waker()` to receive a notification when the
        /// object becomes writable or is closed.
        fn poll_vectored_write(&mut self, cx: &mut task::Context, vec: &[&IoVec])
            -> Poll<usize, Error>
        {
            if let Some(ref first_iovec) = vec.get(0) {
                self.poll_write(cx, &*first_iovec)
            } else {
                // `vec` is empty.
                return Ok(Async::Ready(0));
            }
        }

        /// Attempt to flush the object, ensuring that all intermediately
        /// buffered contents reach their destination.
        ///
        /// On success, returns `Ok(Async::Ready(()))`.
        ///
        /// If flushing is incomplete, this function returns `Ok(Async::Pending)`
        /// and arranges for `cx.waker()` to receive a notification when the
        /// object can make progress towards flushing.
        fn poll_flush(&mut self, cx: &mut task::Context) -> Poll<(), Error>;

        /// Attempt to close the object.
        ///
        /// On success, returns `Ok(Async::Ready(()))`.
        ///
        /// If closing is incomplete, this function returns `Ok(Async::Pending)`
        /// and arranges for `cx.waker()` to receive a notification when the
        /// object can make progress towards closing.
        fn poll_close(&mut self, cx: &mut task::Context) -> Poll<(), Error>;
    }

    macro_rules! deref_async_read {
        () => {
            unsafe fn initializer(&self) -> Initializer {
                (**self).initializer()
            }

            fn poll_read(&mut self, cx: &mut task::Context, buf: &mut [u8])
                -> Poll<usize, Error>
            {
                (**self).poll_read(cx, buf)
            }

            fn poll_vectored_read(&mut self, cx: &mut task::Context, vec: &mut [&mut IoVec])
                -> Poll<usize, Error>
            {
                (**self).poll_vectored_read(cx, vec)
            }
        }
    }

    impl<T: ?Sized + AsyncRead> AsyncRead for Box<T> {
        deref_async_read!();
    }

    impl<'a, T: ?Sized + AsyncRead> AsyncRead for &'a mut T {
        deref_async_read!();
    }

    /// `unsafe` because the `StdIo::Read` type must not access the buffer
    /// before reading data into it.
    macro_rules! unsafe_delegate_async_read_to_stdio {
        () => {
            unsafe fn initializer(&self) -> Initializer {
                Initializer::nop()
            }

            fn poll_read(&mut self, _: &mut task::Context, buf: &mut [u8])
                -> Poll<usize, Error>
            {
                Ok(Async::Ready(StdIo::Read::read(self, buf)?))
            }
        }
    }

    impl<'a> AsyncRead for &'a [u8] {
        unsafe_delegate_async_read_to_stdio!();
    }

    impl AsyncRead for StdIo::Repeat {
        unsafe_delegate_async_read_to_stdio!();
    }

    impl<T: AsRef<[u8]>> AsyncRead for StdIo::Cursor<T> {
        unsafe_delegate_async_read_to_stdio!();
    }

    macro_rules! deref_async_write {
        () => {
            fn poll_write(&mut self, cx: &mut task::Context, buf: &[u8])
                -> Poll<usize, Error>
            {
                (**self).poll_write(cx, buf)
            }

            fn poll_vectored_write(&mut self, cx: &mut task::Context, vec: &[&IoVec])
                -> Poll<usize, Error>
            {
                (**self).poll_vectored_write(cx, vec)
            }

            fn poll_flush(&mut self, cx: &mut task::Context) -> Poll<(), Error> {
                (**self).poll_flush(cx)
            }

            fn poll_close(&mut self, cx: &mut task::Context) -> Poll<(), Error> {
                (**self).poll_close(cx)
            }
        }
    }

    impl<T: ?Sized + AsyncWrite> AsyncWrite for Box<T> { 
        deref_async_write!();
    }

    impl<'a, T: ?Sized + AsyncWrite> AsyncWrite for &'a mut T {
        deref_async_write!();
    }

    macro_rules! delegate_async_write_to_stdio {
        () => {
            fn poll_write(&mut self, _: &mut task::Context, buf: &[u8])
                -> Poll<usize, Error>
            {
                Ok(Async::Ready(StdIo::Write::write(self, buf)?))
            }

            fn poll_flush(&mut self, _: &mut task::Context) -> Poll<(), Error> {
                Ok(Async::Ready(StdIo::Write::flush(self)?))
            }

            fn poll_close(&mut self, cx: &mut task::Context) -> Poll<(), Error> {
                self.poll_flush(cx)
            }
        }
    }

    impl<'a> AsyncWrite for StdIo::Cursor<&'a mut [u8]> {
        delegate_async_write_to_stdio!();
    }

    impl AsyncWrite for StdIo::Cursor<Vec<u8>> {
        delegate_async_write_to_stdio!();
    }

    impl AsyncWrite for StdIo::Cursor<Box<[u8]>> {
        delegate_async_write_to_stdio!();
    }

    impl AsyncWrite for StdIo::Sink {
        delegate_async_write_to_stdio!();
    }
}
