# mpmc-bus

This is inspired by https://github.com/jonhoo/bus. Unfortunately `bus` doesn't provide an easy API to do multiple producers, and it also doesn't have an easy way to dynamically add subscribers to the bus at the level of another subscriber, so this is a wrapper around `bus` to provide that functionality.