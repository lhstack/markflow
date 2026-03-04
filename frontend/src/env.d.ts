/// <reference types="vite/client" />

declare module '*.vue' {
  import type { DefineComponent } from 'vue'
  const component: DefineComponent<{}, {}, any>
  export default component
}

declare module '@kangc/v-md-editor' {
  const VMdEditor: any
  export default VMdEditor
}

declare module '@kangc/v-md-editor/*' {
  const value: any
  export default value
}

declare module 'codemirror' {
  const Codemirror: any
  export default Codemirror
}

declare module 'codemirror/mode/*'
declare module 'codemirror/addon/*'
