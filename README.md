### Fast Approximate Broadcast

Implementation of a gossip protocol

TODO list:
- Write a description
- Implement receiver filtering
- Implement receiver message output queue

Design:
- (TODO) Identity given by public key, address given by TCP/IP (maybe UDP later?) port/address pair, 1 identity per address at a time
- (TODO) Messages buffered as a socket stream
- Separate broadcaster/listener, broadcaster does not need to listen/relay messages