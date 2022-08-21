import asyncio
import mycelial_bindings
import random

from mycelial.transport.relay import V0Alpha


class Mycelial:
    def __init__(self, peer_id=None, transport=V0Alpha):
        if peer_id is None:
            peer_id = random.getrandbits(64)
        self.peer_id = peer_id
        self.crdt = mycelial_bindings.List(self.peer_id)
        self.transport = transport()
        self._task = asyncio.create_task(
            self.transport.spawn(self.peer_id, self.crdt)
        )

    def append(self, value):
        self.crdt.append(value)

    def delete(self, index):
        self.crdt.delete(index)

    def insert(self, index, value):
        self.crdt.insert(index, value)

    def set_on_apply(self, cb):
        self.crdt.set_on_apply(cb)

    def unset_on_apply(self):
        self.crdt.unset_on_update()

    def to_vec(self):
        return self.crdt.to_vec()

    def vclock(self):
        return self.crdt.vclock()

    def vclock_diff(self, vclock):
        return self.crdt.vclock_diff(vclock)

    def diff(self, vclock):
        return self.crdt.diff(vclock)

    def apply(self, diff):
        self.crdt.apply(diff)

    def __del__(self):
        if self._task is not None:
            self._task.cancel()
