<template>
  <div>
    <div v-if="loading">
      <mwc-circular-progress indeterminate></mwc-circular-progress>
    </div>
    <div v-else>
      <div id="content" style="display: flex; flex-direction: column; flex: 1;">
        <h2>EDIT ME! Add the components of your app here.</h2>
        
        <span>Look in the <code>ui/src/DNA/ZOME</code> folders for UI elements that are generated with <code>hc scaffold entry-type</code>, <code>hc scaffold collection</code> and <code>hc scaffold link-type</code> and add them here as appropriate.</span>
        
        <span>For example, if you have scaffolded a "todos" dna, a "todos" zome, a "todo_item" entry type, and a collection called "all_todos", you might want to add an element here to create and list your todo items, with the generated <code>ui/src/todos/todos/AllTodos.vue</code> and <code>ui/src/todos/todos/CreateTodo.vue</code> elements.</span>
          
        <span>So, to use those elements here:</span>
        <ol>
          <li>Import the elements with:
          <pre>
import AllTodos from './todos/todos/AllTodos.vue';
import CreateTodo from './todos/todos/CreateTodo.vue';
          </pre>
          </li>
          <li>Add it into the subcomponents for the `App` component: 
            <pre>
export default defineComponent({
  components: {
    // Add your subcomponents here
    AllTodos,
    CreateTodo
  },
  ...
            </pre>
          </li>
          <li>Replace this "EDIT ME!" section with <code>&lt;CreateTodo&gt;&lt;/CreateTodo&gt;&lt;AllTodos&gt;&lt;/AllTodos&gt;</code>.</li>
        </ol>
      </div>
    </div>
  </div>
</template>
<script lang="ts">
import { defineComponent, computed } from 'vue';
import WebSdk from "@holo-host/web-sdk";
import type { AgentState } from "@holo-host/web-sdk";
import { AppClient, AppWebsocket, HolochainError } from '@holochain/client';
import '@material/mwc-circular-progress';

export default defineComponent({
  components: {
    // Add your subcomponents here
  },
  data(): {
    client: any;
    loading: boolean;
    error: HolochainError | undefined;
    IS_HOLO: boolean;
  } {
    return {
      client: undefined,
      loading: true,
      error: undefined,
      IS_HOLO: ["true", "1", "t"].includes(import.meta.env.VITE_APP_IS_HOLO?.toLowerCase()),
    };
  },
  async mounted() {
    // We pass an unused string as the url because it will dynamically be replaced in launcher environments
    try {
      this.loading = true;
      if (this.IS_HOLO) {
        const client: WebSdk = await WebSdk.connect({
          chaperoneUrl: import.meta.env.VITE_APP_CHAPERONE_URL,
          authFormCustomization: {
            appName: "humm-earth-core-happ",
          },
        });
        client.on("agent-state", (agent_state: AgentState) => {
          this.loading = !agent_state.isAvailable;
        });
        this.client = client;
      } else {
        this.client = await AppWebsocket.connect();
      }
    } catch (e) {
      this.error = e as HolochainError;
    } finally {
      this.loading = false;
    }
  },
  provide() {
    return {
      client: computed(() => this.client),
    };
  },
});
</script>
