============
Rotor Stream
============

:Status: Alpha
:Dependencies: rotor_, mio_, netbuf_

.. _rotor: http://github.com/tailhook/rotor
.. _mio: https://github.com/carllerche/mio
.. _netbuf: https://github.com/tailhook/netbuf

A stream abstraction based on MIO. Features:

* State machine-based implementation (as usually in rotor_)
* Uses netbuf_ for buffering, buffer has contiguous data slice (easy parsing)
* Input data abstractions: read-x-bytes, read-until-delimiter
* Perfect for request-reply style protocols
* Independent of whether it's client or server, tcp or unix sockets
* (TODO) should work on top of SSL later
