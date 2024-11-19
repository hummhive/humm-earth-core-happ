<template>
  <mwc-snackbar ref="create-error"></mwc-snackbar>

  <div style="display: flex; flex-direction: column">
    <span style="font-size: 18px">Create Encrypted Content</span>
  
  
    <mwc-button 
      raised
      label="Create Encrypted Content"
      :disabled="!isEncryptedContentValid"
      @click="createEncryptedContent"
    ></mwc-button>
  </div>
</template>
<script lang="ts">
import { defineComponent, inject, ComputedRef } from 'vue';
import { AppClient, Record, AgentPubKey, EntryHash, ActionHash, DnaHash } from '@holochain/client';
import { EncryptedContent } from './types';
import '@material/mwc-button';
import '@material/mwc-icon-button';
import '@material/mwc-snackbar';
import { Snackbar } from '@material/mwc-snackbar';

export default defineComponent({
  data(): {
  } {
    return { 
    }
  },

  props: {    id: {
      type: null,
      required: true
    },
    entryType: {
      type: null,
      required: true
    },
    bytes: {
      type: null,
      required: true
    },
  },
  computed: {
    isEncryptedContentValid() {
    return true;
    },
  },
  mounted() {
    if (this.id === undefined) {
      throw new Error(`The id input is required for the CreateEncryptedContent element`);
    }
    if (this.entryType === undefined) {
      throw new Error(`The entryType input is required for the CreateEncryptedContent element`);
    }
    if (this.bytes === undefined) {
      throw new Error(`The bytes input is required for the CreateEncryptedContent element`);
    }
  },
  methods: {
    async createEncryptedContent() {
      const encryptedContent: EncryptedContent = { 
        id: this.id!,

        content_type: this.entryType!,

        bytes: this.bytes as Array<number>,
      };

      try {
        const record: Record = await this.client.callZome({
          cap_secret: null,
          role_name: 'humm_earth_core',
          zome_name: 'content',
          fn_name: 'create_encrypted_content',
          payload: encryptedContent,
        });
        this.$emit('encrypted-content-created', record.signed_action.hashed.hash);
      } catch (e: any) {
        const errorSnackbar = this.$refs['create-error'] as Snackbar;
        errorSnackbar.labelText = `Error creating the encrypted content: ${e.data.data}`;
        errorSnackbar.show();
      }
    },
  },
  emits: ['encrypted-content-created'],
  setup() {
    const client = (inject('client') as ComputedRef<AppClient>).value;
    return {
      client,
    };
  },
})
</script>
