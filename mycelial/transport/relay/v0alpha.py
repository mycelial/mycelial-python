# Transport implementation for mycelial v0alpha relay


import asyncio
import websockets
import json


class Protocol:
    def __init__(self, identity, topic):
        self.identity = f"{identity}"
        self.topic = topic

    def sync(self, payload):
        return json.dumps({
            'identity': self.identity,
            'kind': 'sync',
            'payload': payload,
            'topic': self.topic,
            'version': 'v0alpha',
        })

    def diff(self, payload):
        return json.dumps({
            'identity': self.identity,
            'kind': 'diff',
            'payload': payload,
            'topic': self.topic,
            'version': 'v0alpha',
        })


class V0Alpha:
    def __init__(
        self,
        url="wss://v0alpha-relay.fly.dev/v0alpha",
        topic="python-tpc"
    ):
        self.url = url
        self.crdt = None
        self.task = None
        self.topic = topic
        self.protocol = None
        self.queue = asyncio.Queue()

    async def receiver_loop(self, connection):
        while True:
            try:
                msg = json.loads(await connection.recv())
                if msg['kind'] == "sync":
                    (send_diff, send_vclock) = self.crdt.vclock_diff(
                        msg['payload']
                    )
                    if send_vclock:
                        vclock = self.protocol.sync(self.crdt.vclock())
                        await connection.send(vclock)

                    if send_diff:
                        diff = self.crdt.diff(msg['payload'])
                        await connection.send(self.protocol.diff(diff))

                elif msg['kind'] == "diff":
                    try:
                        self.crdt.apply(msg['payload'])
                    # FIXME: properly detect desync exception
                    except Exception as e:
                        await connection.send(
                            self.protocol.sync(self.crdt.vclock())
                        )
                        raise e
                else:
                    pass
            except websockets.ConnectionClosed:
                break
            except Exception as e:
                raise e

    async def sender_loop(self, connection):
        await connection.send(self.protocol.sync(self.crdt.vclock()))
        while True:
            try:
                update = await self.queue.get()
                await connection.send(self.protocol.diff(update))
            except websockets.ConnectionClosed:
                break
            except Exception as e:
                raise e

    async def loop(self):
        self.crdt.set_on_update(self.queue.put_nowait)
        connection = None
        while True:
            try:
                connection = await websockets.connect(self.url)
                r = asyncio.create_task(
                    self.receiver_loop(connection),
                    name='received'
                )
                s = asyncio.create_task(
                    self.sender_loop(connection),
                    name='sender'
                )
                done, pending = await asyncio.wait(
                    [r, s], return_when=asyncio.FIRST_COMPLETED
                )
                for task in pending:
                    task.cancel()
                self.queue._queue.clear()

            except websockets.ConnectionClosed:
                pass

            except Exception as e:
                raise e

            await asyncio.sleep(3)

    async def spawn(self, identity, crdt):
        if self.task is None:
            self.crdt = crdt
            self.protocol = Protocol(identity, self.topic)
            self.task = asyncio.create_task(self.loop())
        else:
            raise Exception("spawn allowed only once")

    def __del__(self):
        if self.task is not None:
            self.task.cancel()
        if self.crdt is not None:
            self.crdt.unset_on_update()
