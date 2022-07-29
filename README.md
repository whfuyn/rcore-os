# rcore-os

参考[rCore-Tutorial-V3](https://rcore-os.github.io/rCore-Tutorial-Book-v3/)做的内核。


这里每一步都是按照我自己的理解来做的，可能和教程里的代码有比较多的差异。

- 从源码编译`rustsbi-qemu`，而不使用预编译的版本。
- 按需求写的Makefile。
- 内核放在高位地址，而不是用单独的地址空间（单页表）
- 通过单独的loader(base 0x80200000)加载内核(base 0xffffffffc0000000)，不依赖编译器对跳转的编译结果

开发环境是WSL2，没有在其它平台测试过。

## 用法

获取`rustsbi-qemu`。
```
$ git submodule update --init
```

编译`rcore-os`并在`qemu`上运行。
```
$ make run
```

同上，但是会等待gdb接入。
```
$ make debug
```

启动gdb并接入。
```
$ make gdb
```

## 踩坑记录

> error: `sys_common::condvar::Condvar::new` is not yet stable as a const fn

https://github.com/rust-lang/rust/pull/98457

照着这个pr改本地的std，把报错里提到的都加上就可以了。
