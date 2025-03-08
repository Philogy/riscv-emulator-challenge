import bisect
from collections import Counter

PAGE = 1024
CUTOFF = 0x10000

MAX_GAP = 4


def find_group_index(groups, x, PAGE):
    """
    Binary search to find appropriate group index.
    Returns index of group where abs(start - x) < PAGE or -1 if not found.
    """
    if not groups:
        return -1

    # Get all starts for binary search
    starts = [g[0] for g in groups]

    # Find closest position where we might insert x
    pos = bisect.bisect_left(starts, x)

    # Check the potential positions
    candidates = []

    # Check position at pos (if it exists)
    if pos < len(groups):
        if abs(groups[pos][0] - x) < PAGE:
            candidates.append(pos)

    # Check position before pos (if it exists)
    if pos > 0:
        if abs(groups[pos-1][0] - x) < PAGE:
            candidates.append(pos-1)

    # Return the first matching group (prioritizing earlier groups)
    return min(candidates) if candidates else -1


def process_raw_text():
    with open('./keys.txt', 'r') as f:
        nums = [
            int(
                ''.join(filter(str.isdigit, s))
            )
            for s in f.read().split(',')
        ]
        out_buf = [0] * (len(nums) * 4)
        for i, x in enumerate(nums):
            out_buf[i*4:(i+1)*4] = x.to_bytes(4, 'little')
        with open('keys.hex', 'wb') as f:
            f.write(bytes(out_buf))


def get_groups(nums: list[int], gap: int) -> list[list[int]]:
    groups = [[nums[0]]]

    for x in nums[1:]:
        last_group = groups[-1]
        if x - last_group[-1] > gap:
            groups.append([x])
        else:
            last_group.append(x)

    return groups


def main():
    with open('keys.hex', 'rb') as f:
        raw_nums = f.read()
        nums = [
            int.from_bytes(raw_nums[i*4:(i+1)*4], 'little')
            for i in range(len(raw_nums) // 4)
        ]

    # print(f'[{", ".join(map(lambda x: hex(x)[2:], nums[:100]))}]')

    low = []
    high = []
    for x in nums:
        if x < CUTOFF:
            low.append(x)
        else:
            assert x % 4 == 0
            high.append(CUTOFF + (x - CUTOFF) // 4)

    for max_dist in (1, 2, 4, 8, 16, 32, 64):
        groups = get_groups(high, max_dist)
        gaps = sum(
            y - x - 1
            for g in groups
            for x, y in zip(g, g[1:])
        )

        total = len(high) + gaps
        print(f'{max_dist}: {len(groups)} ({gaps} - {gaps / total:.2%})')


if __name__ == '__main__':
    main()
