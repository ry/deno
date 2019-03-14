
const sharedBytes = new Uint8Array(libdeno.shared);
const shared32 = new Int32Array(libdeno.shared);

const MAX_RECORDS = 100;
const INDEX_NUM_RECORDS = 0;
const INDEX_NUM_SHIFTED_OFF = 1;
const INDEX_HEAD = 2;
const INDEX_OFFSETS = 3;
const INDEX_RECORDS = 3 + MAX_RECORDS;
const HEAD_INIT = 4 * INDEX_RECORDS;

function assert(cond) {
  if (!cond) { throw Error("assert"); }
}

function reset() {
  shared32.fill(0, 0, INDEX_RECORDS);
  shared32[INDEX_HEAD] = HEAD_INIT;
}

function head() {
  return shared32[INDEX_HEAD];
}

function setEnd(index, end) {
  shared32[INDEX_OFFSETS + index] = end;
}

function getEnd(index) {
  if (index < numRecords()) {
    return shared32[INDEX_OFFSETS + index];
  } else {
    return null;
  }
}

function getOffset(index) {
  if (index < numRecords()) {
    if (index == 0) {
      return HEAD_INIT;
    } else {
      return shared32[INDEX_OFFSETS + index - 1];
    }
  } else {
    return null;
  }
}

function numRecords() {
  return shared32[INDEX_NUM_RECORDS];
}

function activeRecords() {
  return shared32[INDEX_NUM_RECORDS] - shared32[INDEX_NUM_SHIFTED_OFF];
}

function push(buf) {
  let off = head();
  let end = off + buf.byteLength;
  let index = numRecords();
  if (end > shared32.byteLength) {
    return false;
  }
  setEnd(index, end);
  assert(end - off == buf.byteLength);
  sharedBytes.set(buf, off);
  shared32[INDEX_NUM_RECORDS] += 1;
  shared32[INDEX_HEAD] = end;
  return true
}

/// Returns null if empty.
function shift() {
  let i = shared32[INDEX_NUM_SHIFTED_OFF];
  if (i >= numRecords()) {
    return null;
  }
  let off = getOffset(i);
  let end = getEnd(i);
  // println(`shift ${off} ${end}`);
  shared32[INDEX_NUM_SHIFTED_OFF] += 1;
  return sharedBytes.subarray(off, end);
}

function println(s) {
  libdeno.print(`${s}\n`);
}

function test() {
  // println("hello .. shared_queue.js");

  let h = head();
  assert(h > 0);

  let r = new Uint8Array([1, 2, 3, 4, 5]);
  let len = r.byteLength + h;
  assert(push(r));
  assert(head() == len);

  r = new Uint8Array([6, 7]);
  assert(push(r));

  r = new Uint8Array([8, 9, 10, 11]);
  assert(push(r));
  assert(numRecords() == 3);
  assert(activeRecords() == 3);

  r = shift();
  assert(r.byteLength == 5);
  assert(r[0] == 1);
  assert(r[1] == 2);
  assert(r[2] == 3);
  assert(r[3] == 4);
  assert(r[4] == 5);
  assert(numRecords() == 3);
  assert(activeRecords() == 2);

  r = shift();
  assert(r.byteLength == 2);
  assert(r[0] == 6);
  assert(r[1] == 7);
  assert(numRecords() == 3);
  assert(activeRecords() == 1);

  r = shift();
  assert(r.byteLength == 4);
  assert(r[0] == 8);
  assert(r[1] == 9);
  assert(r[2] == 10);
  assert(r[3] == 11);
  assert(numRecords() == 3);
  assert(activeRecords() == 0);

  assert(shift() == null);
  assert(shift() == null);
  assert(numRecords() == 3);
  assert(activeRecords() == 0);

  reset();
  assert(numRecords() == 0);
  assert(activeRecords() == 0);
}
