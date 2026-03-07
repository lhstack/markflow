<template>
  <span class="tree-node-glyph" :class="[`kind-${kind}`, { active, expanded }]">
    <svg
      v-if="kind === 'dir'"
      :width="size"
      :height="size"
      viewBox="0 0 20 20"
      fill="none"
      aria-hidden="true"
    >
      <path
        d="M2.4 5.4C2.4 4.295 3.295 3.4 4.4 3.4h3.14c.53 0 1.038.21 1.414.585l.792.79c.15.15.353.235.565.235H15.6c1.105 0 2 .895 2 2v1.01H2.4V5.4Z"
        :fill="folderBack"
      />
      <path
        :d="folderFrontPath"
        :fill="folderFront"
      />
      <path
        d="M2.95 7.55h14.1"
        :stroke="folderAccent"
        stroke-width="0.9"
        stroke-linecap="round"
        opacity="0.6"
      />
    </svg>

    <svg
      v-else
      :width="size"
      :height="size"
      viewBox="0 0 20 20"
      fill="none"
      aria-hidden="true"
    >
      <path
        d="M5.4 2.6h5.05c.53 0 1.04.21 1.415.586l2.55 2.55c.375.375.585.884.585 1.414v7.45c0 1.105-.895 2-2 2H5.4c-1.105 0-2-.895-2-2v-10c0-1.105.895-2 2-2Z"
        :fill="docPage"
        :stroke="docStroke"
        stroke-width="1"
      />
      <path
        d="M10.95 2.95v2.4c0 .884.716 1.6 1.6 1.6h2.4"
        :stroke="docStroke"
        stroke-width="1"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
      <path
        d="M6.4 6.1h1.3"
        :stroke="docAccent"
        stroke-width="1.6"
        stroke-linecap="round"
      />
      <path
        d="M6.35 9.1h6.8M6.35 11.45h6.05M6.35 13.8h4.6"
        :stroke="docLine"
        stroke-width="1.1"
        stroke-linecap="round"
        opacity="0.9"
      />
    </svg>
  </span>
</template>

<script setup lang="ts">
import { computed } from 'vue'

const props = withDefaults(defineProps<{
  kind: 'dir' | 'doc'
  expanded?: boolean
  active?: boolean
  size?: number
}>(), {
  expanded: false,
  active: false,
  size: 18,
})

const folderBack = computed(() => {
  if (props.expanded) return props.active ? '#6f9f4d' : '#7ea95b'
  return props.active ? '#9a7b37' : '#b18e45'
})

const folderFront = computed(() => {
  if (props.expanded) return props.active ? '#96c766' : '#acd57c'
  return props.active ? '#d8b969' : '#e8c979'
})

const folderAccent = computed(() => (props.expanded ? '#f4fbec' : '#fff5d6'))

const folderFrontPath = computed(() =>
  props.expanded
    ? 'M2.1 7.3h15.8l-1.1 5.5A2 2 0 0 1 14.84 14.4H5.15a2 2 0 0 1-1.95-1.56L1.92 7.95A.55.55 0 0 1 2.1 7.3Z'
    : 'M2.4 7.1h15.2v5.5c0 1.105-.895 2-2 2H4.4c-1.105 0-2-.895-2-2V7.1Z'
)

const docPage = computed(() => (props.active ? '#f8fff2' : '#f7faf3'))
const docStroke = computed(() => (props.active ? '#5f8d55' : '#8fa082'))
const docAccent = computed(() => (props.active ? '#79a65f' : '#95b67e'))
const docLine = computed(() => (props.active ? '#6d8a60' : '#8d9a88'))
</script>

<style scoped>
.tree-node-glyph {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  line-height: 0;
}
</style>
