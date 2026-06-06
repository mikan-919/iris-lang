# First
私は関数型かつパイプラインで、型が強めで、Python,Typescriptの様に書き易いプログラミング言語を作りたい。

まず、パイプライン。
```
value
:: process()
  @catch
    :: defaultValue

// or
:catch (
  :: catchedProcess()
  :: other()
)
:if (x -> x === 10) //ここで$xは現在のパイプライン主体を表す
:then  (
    :: processes()
    :: other2()
)
:else  otherSingleValue
::     trim()
```
こういう感じ。
変数の扱いは基本的にRustの所有権的な感じだが、それを仕様を決める側に強制して、使う側は考えずに使えるようにしたい。
Zigのような管理はやめておこう。いや、うーん、コンパイルタイムで
```
!impl(Trait)
type Type = {
  tmp: string
}
```
こういうのを導入するから、
```
!unfree()
let x = 10
```
とかがあってもいいのかもだけど、その場合チェックがきつい

それから函数も
```
fn fn_name(arg:string) -> number
  :: pipeline
```
みたいにできたり、
```
fn fn_name(...args:string[]) -> other {
  
}
```
という普通の書き方もできてほしい

とりあえず、基本文法を考えよう。
```
use
- module.file
- module2.file2 as fnA

fn main()-> int {
  let x :number = 21
  let processed =
    fnX(x)
    :: fnA() 
      :catch module.file.fn2() 
    :: trim()
    :: other() @unwrap

  let x :number@mut@ref = 21
  let y = copy(x)
  changeValue(x)
  put(`x is ${x===y?'':'not'} same.`)

  put(`${processed :: to_string()}`)
}
```

コンポーネントのほう
```
Card(
  {
    class: "flex ..."
  },
  ::Title("")
)
```
